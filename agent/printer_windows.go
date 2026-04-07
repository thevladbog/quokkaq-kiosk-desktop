//go:build windows

package main

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

type psPrinter struct {
	Name    string `json:"Name"`
	Default bool   `json:"Default"`
}

func listPrintersOS() ([]PrinterInfo, error) {
	cmd := exec.Command(
		"powershell", "-NoProfile", "-NonInteractive", "-Command",
		"Get-Printer | Select-Object Name,Default | ConvertTo-Json -Compress -Depth 4",
	)
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("powershell Get-Printer: %w", err)
	}
	trimmed := strings.TrimSpace(string(out))
	if trimmed == "" {
		return []PrinterInfo{}, nil
	}
	var list []PrinterInfo
	switch trimmed[0] {
	case '[':
		var arr []psPrinter
		if err := json.Unmarshal([]byte(trimmed), &arr); err != nil {
			return nil, fmt.Errorf("parse printers json: %w", err)
		}
		for _, p := range arr {
			if p.Name == "" {
				continue
			}
			list = append(list, PrinterInfo{Name: p.Name, IsDefault: p.Default})
		}
	default:
		var one psPrinter
		if err := json.Unmarshal([]byte(trimmed), &one); err != nil {
			return nil, fmt.Errorf("parse printer json: %w", err)
		}
		if one.Name != "" {
			list = append(list, PrinterInfo{Name: one.Name, IsDefault: one.Default})
		}
	}
	return list, nil
}

var (
	winspool               = windows.NewLazySystemDLL("winspool.drv")
	procOpenPrinterW       = winspool.NewProc("OpenPrinterW")
	procClosePrinter       = winspool.NewProc("ClosePrinter")
	procStartDocPrinterW   = winspool.NewProc("StartDocPrinterW")
	procEndDocPrinter      = winspool.NewProc("EndDocPrinter")
	procStartPagePrinter   = winspool.NewProc("StartPagePrinter")
	procEndPagePrinter     = winspool.NewProc("EndPagePrinter")
	procWritePrinter       = winspool.NewProc("WritePrinter")
)

type docInfo1 struct {
	DocName    *uint16
	OutputFile *uint16
	Datatype   *uint16
}

func printSystemOS(queue string, raw []byte) error {
	if queue == "" {
		return fmt.Errorf("empty printer queue name")
	}
	if len(raw) == 0 {
		return fmt.Errorf("empty payload")
	}

	var h syscall.Handle
	pName, err := windows.UTF16PtrFromString(queue)
	if err != nil {
		return err
	}
	r0, _, e := procOpenPrinterW.Call(
		uintptr(unsafe.Pointer(pName)),
		uintptr(unsafe.Pointer(&h)),
		0,
	)
	if r0 == 0 {
		return fmt.Errorf("OpenPrinterW: %v", e)
	}
	defer procClosePrinter.Call(uintptr(h))

	docTitle, _ := windows.UTF16PtrFromString("QuokkaQ Kiosk")
	rawDatatype, _ := windows.UTF16PtrFromString("RAW")
	di := docInfo1{
		DocName:    docTitle,
		OutputFile: nil,
		Datatype:   rawDatatype,
	}

	job, _, e := procStartDocPrinterW.Call(
		uintptr(h),
		1,
		uintptr(unsafe.Pointer(&di)),
	)
	if job == 0 {
		return fmt.Errorf("StartDocPrinterW: %v", e)
	}

	r1, _, e := procStartPagePrinter.Call(uintptr(h))
	if r1 == 0 {
		_, _, _ = procEndDocPrinter.Call(uintptr(h))
		return fmt.Errorf("StartPagePrinter: %v", e)
	}

	var written uint32
	r2, _, e := procWritePrinter.Call(
		uintptr(h),
		uintptr(unsafe.Pointer(&raw[0])),
		uintptr(len(raw)),
		uintptr(unsafe.Pointer(&written)),
	)
	if r2 == 0 {
		_, _, _ = procEndPagePrinter.Call(uintptr(h))
		_, _, _ = procEndDocPrinter.Call(uintptr(h))
		return fmt.Errorf("WritePrinter: %v", e)
	}

	_, _, _ = procEndPagePrinter.Call(uintptr(h))
	_, _, _ = procEndDocPrinter.Call(uintptr(h))
	return nil
}
