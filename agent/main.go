// QuokkaQ kiosk hardware agent: local HTTP API for receipt printers (TCP + system queues).
package main

import (
	"encoding/base64"
	"encoding/json"
	"log"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

const defaultAddr = "127.0.0.1:17431"

type printRequest struct {
	Mode    string `json:"mode"`    // "tcp" | "system"
	Target  string `json:"target"`  // host:port or queue name
	Payload string `json:"payload"` // base64

	// Legacy (same as mode=tcp, target=address)
	Address string `json:"address"`
}

type printersResponse struct {
	Printers []PrinterInfo `json:"printers"`
	Error    string        `json:"error,omitempty"`
}

func main() {
	addr := os.Getenv("QUOKKAQ_AGENT_LISTEN")
	if addr == "" {
		addr = defaultAddr
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/health", handleHealth)
	mux.HandleFunc("/v1/printers", handleListPrinters)
	mux.HandleFunc("/v1/print", handlePrint)

	srv := &http.Server{
		Addr:              addr,
		Handler:           mux,
		ReadHeaderTimeout: 5 * time.Second,
	}
	log.Printf("quokkaq-kiosk-agent listening on http://%s", addr)
	if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		log.Fatal(err)
	}
}

func handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"ok":true}`))
}

func handleListPrinters(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	list, err := listPrintersOS()
	if err != nil {
		out, _ := json.Marshal(printersResponse{Printers: nil, Error: err.Error()})
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(out)
		return
	}
	out, _ := json.Marshal(printersResponse{Printers: list})
	_, _ = w.Write(out)
}

func handlePrint(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var req printRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid json", http.StatusBadRequest)
		return
	}

	mode := strings.ToLower(strings.TrimSpace(req.Mode))
	target := strings.TrimSpace(req.Target)
	if mode == "" && req.Address != "" {
		mode = "tcp"
		target = strings.TrimSpace(req.Address)
	}
	if mode == "" {
		mode = "tcp"
	}
	if req.Payload == "" {
		http.Error(w, "payload required", http.StatusBadRequest)
		return
	}
	raw, err := base64.StdEncoding.DecodeString(req.Payload)
	if err != nil {
		http.Error(w, "invalid base64 payload", http.StatusBadRequest)
		return
	}

	switch mode {
	case "tcp":
		if target == "" {
			http.Error(w, "target required for tcp mode (host:port)", http.StatusBadRequest)
			return
		}
		if err := printTCP(target, raw); err != nil {
			http.Error(w, err.Error(), http.StatusBadGateway)
			return
		}
	case "system":
		if target == "" {
			http.Error(w, "target required for system mode (queue name)", http.StatusBadRequest)
			return
		}
		if err := printSystemOS(target, raw); err != nil {
			http.Error(w, err.Error(), http.StatusBadGateway)
			return
		}
	default:
		http.Error(w, "mode must be tcp or system", http.StatusBadRequest)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write([]byte(`{"ok":true}`))
}

func printTCP(address string, raw []byte) error {
	conn, err := net.DialTimeout("tcp", address, 5*time.Second)
	if err != nil {
		return err
	}
	defer conn.Close()
	if err := conn.SetWriteDeadline(time.Now().Add(10 * time.Second)); err != nil {
		return err
	}
	_, err = conn.Write(raw)
	return err
}
