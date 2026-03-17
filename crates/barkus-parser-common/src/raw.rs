#[derive(Clone)]
pub struct RawGrammar<I> {
    pub rules: Vec<RawRule<I>>,
}

#[derive(Clone)]
pub struct RawRule<I> {
    pub name: String,
    pub alternatives: Vec<RawAlternative<I>>,
    pub line: usize,
    pub col: usize,
}

#[derive(Clone)]
pub struct RawAlternative<I> {
    pub items: Vec<I>,
}

#[derive(Clone, Copy)]
pub enum RawQuantifier {
    Optional,
    ZeroOrMore,
    OneOrMore,
}
