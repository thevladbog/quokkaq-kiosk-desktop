package main

// PrinterInfo is returned by GET /v1/printers.
type PrinterInfo struct {
	Name      string `json:"name"`
	IsDefault bool   `json:"isDefault"`
}
