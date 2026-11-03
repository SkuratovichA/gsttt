package invoice

type Footer struct{ Note string }

func (f Footer) Render() string { return f.Note }

var suffix = loadSuffix()

func init() {
	register("footer")
}
