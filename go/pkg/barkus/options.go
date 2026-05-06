package barkus

// Option works on any generator (SQL, any grammar).
type Option interface {
	SQLOption
	GrammarOption
}

// SQLOption configures a SQLGenerator.
type SQLOption interface {
	applySQL(*sqlConfig)
}

// GrammarOption configures a Generator built from grammar source.
type GrammarOption interface {
	applyGrammar(*grammarConfig)
}

type commonConfig struct {
	seed          uint64
	validityMode  *ValidityMode
	maxDepth      *uint32
	maxTotalNodes *uint32
}

type commonOption func(*commonConfig)

func (o commonOption) applySQL(c *sqlConfig)         { o(&c.commonConfig) }
func (o commonOption) applyGrammar(c *grammarConfig) { o(&c.commonConfig) }

// WithSeed sets the RNG seed for deterministic generation. 0 = random.
func WithSeed(seed uint64) Option {
	return commonOption(func(c *commonConfig) { c.seed = seed })
}

// WithValidityMode pins the generator to a specific ValidityMode
// (Strict / NearValid / Havoc). Default is Strict.
func WithValidityMode(m ValidityMode) Option {
	return commonOption(func(c *commonConfig) { c.validityMode = &m })
}

// WithMaxDepth caps the maximum derivation depth (default 30).
func WithMaxDepth(depth uint32) Option {
	return commonOption(func(c *commonConfig) { c.maxDepth = &depth })
}

// WithMaxTotalNodes caps the total AST node count (default 20_000).
func WithMaxTotalNodes(n uint32) Option {
	return commonOption(func(c *commonConfig) { c.maxTotalNodes = &n })
}
