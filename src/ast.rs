use crate::error::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    FnDef(FnDef),
    Assign {
        name: String,
        annotation: Option<SchemaExpr>,
        value: Expr,
        span: Span,
    },
    If {
        cond: Expr,
        then_block: Vec<Stmt>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    For {
        pattern: Pattern,
        iter: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Assert {
        expr: Expr,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::FnDef(node) => node.span,
            Stmt::Assign { span, .. }
            | Stmt::If { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Assert { span, .. }
            | Stmt::Expr { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<FnParam>,
    pub return_schema: Option<SchemaExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub schema: Option<SchemaExpr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Name(String),
    Tuple(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    Nil(Span),
    Var(String, Span),
    List(Vec<Expr>, Span),
    Tuple(Vec<Expr>, Span),
    Object(Vec<(String, Expr)>, Span),

    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Member {
        target: Box<Expr>,
        name: String,
        span: Span,
    },
    TupleIndex {
        target: Box<Expr>,
        index: usize,
        span: Span,
    },
    Prompt(PromptExpr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, span)
            | Expr::Float(_, span)
            | Expr::Bool(_, span)
            | Expr::Str(_, span)
            | Expr::Nil(span)
            | Expr::Var(_, span)
            | Expr::List(_, span)
            | Expr::Tuple(_, span)
            | Expr::Object(_, span) => *span,
            Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Index { span, .. }
            | Expr::Member { span, .. }
            | Expr::TupleIndex { span, .. } => *span,
            Expr::Prompt(prompt) => prompt.span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub struct PromptExpr {
    pub parts: Vec<PromptPart>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PromptPart {
    Text(String),
    Interpolation(Expr),
}

#[derive(Debug, Clone)]
pub enum SchemaExpr {
    Any,
    Int,
    Float,
    Bool,
    String,
    List(Box<SchemaExpr>),
    Tuple(Vec<SchemaExpr>),
    Object(Vec<SchemaField>),
    Union(Vec<SchemaExpr>),
    Optional(Box<SchemaExpr>),
}

#[derive(Debug, Clone)]
pub struct SchemaField {
    pub name: String,
    pub schema: SchemaExpr,
}
