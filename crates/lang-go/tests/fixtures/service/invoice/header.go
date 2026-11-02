package invoice

// Header opens an invoice.
type Header struct {
	Number string
	Lines  []Line
}

func NewHeader(number string) *Header {
	return &Header{Number: number}
}

func (h *Header) Title() string {
	return "Invoice " + h.Number + suffix
}
