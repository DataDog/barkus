package barkus

// Option configures any generator. Anything that satisfies both
// SQLOption and GrammarOption can be passed to NewSQLGenerator,
// NewGeneratorWithOptions, or DecodeWithOptions.
type Option interface {
	SQLOption
	GrammarOption
}

// SQLOption configures a SQLGenerator. Use the shared With* helpers
// (WithSeed, WithValidityMode, WithMaxDepth, WithMaxTotalNodes) or
// the SQL-only WithSchema / WithSchemaJSON.
type SQLOption interface {
	applySQL(*sqlConfig)
}

// GrammarOption configures a Generator built from a grammar source —
// EBNF, ANTLR, or PEG, selected via WithFormat. Use the shared With*
// helpers or the grammar-only WithFormat.
type GrammarOption interface {
	applyGrammar(*grammarConfig)
}

// commonConfig holds fields shared by sqlConfig and grammarConfig. Both
// configs embed it so the same setters work on either.
type commonConfig struct {
	seed          uint64
	validityMode  *ValidityMode
	maxDepth      *uint32
	maxTotalNodes *uint32
}

// commonOption is an Option that only touches commonConfig fields.
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
