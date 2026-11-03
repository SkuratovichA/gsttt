package invoice

type Line struct {
	Amount int
}

func loadSuffix() string { return min(1, 2) }

func register(name string) {}
