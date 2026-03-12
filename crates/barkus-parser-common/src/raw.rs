pub struct RawGrammar<I> {
    pub rules: Vec<RawRule<I>>,
}

pub struct RawRule<I> {
    pub name: String,
    pub alternatives: Vec<RawAlternative<I>>,
    pub line: usize,
    pub col: usize,
}

pub struct RawAlternative<I> {
    pub items: Vec<I>,
}

#[derive(Clone, Copy)]
pub enum RawQuantifier {
    Optional,
    ZeroOrMore,
    OneOrMore,
}
