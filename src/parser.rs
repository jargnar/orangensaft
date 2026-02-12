use crate::ast::{
    BinaryOp, Expr, FnDef, FnParam, Pattern, Program, PromptExpr, PromptPart, SchemaExpr,
    SchemaField, Stmt, UnaryOp,
};
use crate::error::{SaftError, SaftResult, Span};
use crate::lexer;
use crate::token::{Token, TokenKind};

pub fn parse(tokens: Vec<Token>) -> SaftResult<Program> {
    Parser::new(tokens).parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse_program(&mut self) -> SaftResult<Program> {
        let start = self.current().span;
        let mut stmts = Vec::new();

        while !self.is_eof() {
            self.consume_newlines();
            if self.is_eof() {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }

        let span = if let Some(last) = stmts.last() {
            Span::merge(start, last.span())
        } else {
            start
        };

        Ok(Program { stmts, span })
    }

    fn parse_stmt(&mut self) -> SaftResult<Stmt> {
        if self.match_simple(TokenKind::F) {
            return self.parse_fn_def();
        }
        if self.match_simple(TokenKind::If) {
            return self.parse_if();
        }
        if self.match_simple(TokenKind::For) {
            return self.parse_for();
        }
        if self.match_simple(TokenKind::Ret) {
            return self.parse_return();
        }
        if self.match_simple(TokenKind::Assert) {
            return self.parse_assert();
        }

        if self.is_assign_stmt_start() {
            return self.parse_assign();
        }

        self.parse_expr_stmt()
    }

    fn parse_fn_def(&mut self) -> SaftResult<Stmt> {
        let start = self.previous().span;
        let (name, _) = self.expect_ident("expected function name after 'f'")?;
        self.expect_simple(TokenKind::LParen, "expected '(' after function name")?;

        let mut params = Vec::new();
        if !self.check_simple(&TokenKind::RParen) {
            loop {
                let (param_name, param_span) =
                    self.expect_ident("expected parameter name in function signature")?;
                let schema = if self.match_simple(TokenKind::Colon) {
                    Some(self.parse_schema_expr()?)
                } else {
                    None
                };
                params.push(FnParam {
                    name: param_name,
                    schema,
                    span: param_span,
                });

                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect_simple(TokenKind::RParen, "expected ')' after parameter list")?;

        let return_schema = if self.match_simple(TokenKind::Arrow) {
            Some(self.parse_schema_expr()?)
        } else {
            None
        };

        self.expect_simple(TokenKind::Colon, "expected ':' after function signature")?;
        let body = self.parse_block()?;
        let end = body.last().map(Stmt::span).unwrap_or(start);

        Ok(Stmt::FnDef(FnDef {
            name,
            params,
            return_schema,
            body,
            span: Span::merge(start, end),
        }))
    }

    fn parse_if(&mut self) -> SaftResult<Stmt> {
        let start = self.previous().span;
        let cond = self.parse_expr()?;
        self.expect_simple(TokenKind::Colon, "expected ':' after if condition")?;
        let then_block = self.parse_block()?;
        let mut end = then_block.last().map(Stmt::span).unwrap_or(start);

        let else_block = if self.match_simple(TokenKind::Else) {
            self.expect_simple(TokenKind::Colon, "expected ':' after else")?;
            let block = self.parse_block()?;
            if let Some(last) = block.last() {
                end = last.span();
            }
            Some(block)
        } else {
            None
        };

        Ok(Stmt::If {
            cond,
            then_block,
            else_block,
            span: Span::merge(start, end),
        })
    }

    fn parse_for(&mut self) -> SaftResult<Stmt> {
        let start = self.previous().span;
        let pattern = self.parse_pattern()?;
        self.expect_simple(TokenKind::In, "expected 'in' in for loop")?;
        let iter = self.parse_expr()?;
        self.expect_simple(TokenKind::Colon, "expected ':' after for loop header")?;
        let body = self.parse_block()?;
        let end = body.last().map(Stmt::span).unwrap_or(start);

        Ok(Stmt::For {
            pattern,
            iter,
            body,
            span: Span::merge(start, end),
        })
    }

    fn parse_pattern(&mut self) -> SaftResult<Pattern> {
        let (first, _) = self.expect_ident("expected pattern name in for loop")?;
        if !self.match_simple(TokenKind::Comma) {
            return Ok(Pattern::Name(first));
        }

        let mut names = vec![first];
        loop {
            let (name, _) = self.expect_ident("expected name in tuple destructuring pattern")?;
            names.push(name);
            if !self.match_simple(TokenKind::Comma) {
                break;
            }
        }

        Ok(Pattern::Tuple(names))
    }

    fn parse_return(&mut self) -> SaftResult<Stmt> {
        let start = self.previous().span;
        if self.check_simple(&TokenKind::Newline) {
            let nl = self.advance();
            return Ok(Stmt::Return {
                value: None,
                span: Span::merge(start, nl.span),
            });
        }

        let value = self.parse_expr()?;
        let nl = self.expect_simple(TokenKind::Newline, "expected newline after return")?;
        Ok(Stmt::Return {
            value: Some(value),
            span: Span::merge(start, nl.span),
        })
    }

    fn parse_assert(&mut self) -> SaftResult<Stmt> {
        let start = self.previous().span;
        let expr = self.parse_expr()?;
        let nl = self.expect_simple(TokenKind::Newline, "expected newline after assert")?;
        Ok(Stmt::Assert {
            expr,
            span: Span::merge(start, nl.span),
        })
    }

    fn parse_assign(&mut self) -> SaftResult<Stmt> {
        let (name, name_span) = self.expect_ident("expected assignment target")?;
        let annotation = if self.match_simple(TokenKind::Colon) {
            Some(self.parse_schema_expr()?)
        } else {
            None
        };

        self.expect_simple(TokenKind::Eq, "expected '=' in assignment")?;
        let value = self.parse_expr()?;
        let nl = self.expect_simple(TokenKind::Newline, "expected newline after assignment")?;

        Ok(Stmt::Assign {
            name,
            annotation,
            value,
            span: Span::merge(name_span, nl.span),
        })
    }

    fn parse_expr_stmt(&mut self) -> SaftResult<Stmt> {
        let expr = self.parse_expr()?;
        let nl = self.expect_simple(TokenKind::Newline, "expected newline after expression")?;
        Ok(Stmt::Expr {
            span: Span::merge(expr.span(), nl.span),
            expr,
        })
    }

    fn parse_block(&mut self) -> SaftResult<Vec<Stmt>> {
        self.expect_simple(TokenKind::Newline, "expected newline before block")?;
        self.expect_simple(TokenKind::Indent, "expected indented block")?;

        let mut stmts = Vec::new();
        while !self.check_simple(&TokenKind::Dedent) && !self.is_eof() {
            self.consume_newlines();
            if self.check_simple(&TokenKind::Dedent) {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }

        self.expect_simple(TokenKind::Dedent, "expected end of block")?;
        if stmts.is_empty() {
            return Err(SaftError::with_span(
                "empty block is not allowed",
                self.previous().span,
            ));
        }
        Ok(stmts)
    }

    fn parse_expr(&mut self) -> SaftResult<Expr> {
        self.parse_logic_or()
    }

    fn parse_logic_or(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_logic_and()?;
        while self.match_simple(TokenKind::Or) {
            let right = self.parse_logic_and()?;
            let span = Span::merge(expr.span(), right.span());
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::Or,
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_logic_and(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_equality()?;
        while self.match_simple(TokenKind::And) {
            let right = self.parse_equality()?;
            let span = Span::merge(expr.span(), right.span());
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::And,
                right: Box::new(right),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_comparison()?;

        loop {
            let op = if self.match_simple(TokenKind::EqEq) {
                Some(BinaryOp::Eq)
            } else if self.match_simple(TokenKind::BangEq) {
                Some(BinaryOp::Ne)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_comparison()?;
            let span = Span::merge(expr.span(), right.span());
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn parse_comparison(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_term()?;

        loop {
            let op = if self.match_simple(TokenKind::Lt) {
                Some(BinaryOp::Lt)
            } else if self.match_simple(TokenKind::LtEq) {
                Some(BinaryOp::Le)
            } else if self.match_simple(TokenKind::Gt) {
                Some(BinaryOp::Gt)
            } else if self.match_simple(TokenKind::GtEq) {
                Some(BinaryOp::Ge)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_term()?;
            let span = Span::merge(expr.span(), right.span());
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn parse_term(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_factor()?;

        loop {
            let op = if self.match_simple(TokenKind::Plus) {
                Some(BinaryOp::Add)
            } else if self.match_simple(TokenKind::Minus) {
                Some(BinaryOp::Sub)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_factor()?;
            let span = Span::merge(expr.span(), right.span());
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_unary()?;

        loop {
            let op = if self.match_simple(TokenKind::Star) {
                Some(BinaryOp::Mul)
            } else if self.match_simple(TokenKind::Slash) {
                Some(BinaryOp::Div)
            } else if self.match_simple(TokenKind::Percent) {
                Some(BinaryOp::Mod)
            } else {
                None
            };

            let Some(op) = op else {
                break;
            };

            let right = self.parse_unary()?;
            let span = Span::merge(expr.span(), right.span());
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
                span,
            };
        }

        Ok(expr)
    }

    fn parse_unary(&mut self) -> SaftResult<Expr> {
        if self.match_simple(TokenKind::Minus) {
            let start = self.previous().span;
            let expr = self.parse_unary()?;
            let span = Span::merge(start, expr.span());
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
                span,
            });
        }

        if self.match_simple(TokenKind::Not) {
            let start = self.previous().span;
            let expr = self.parse_unary()?;
            let span = Span::merge(start, expr.span());
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
                span,
            });
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> SaftResult<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_simple(TokenKind::LParen) {
                let mut args = Vec::new();
                if !self.check_simple(&TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if !self.match_simple(TokenKind::Comma) {
                            break;
                        }
                    }
                }
                let end = self.expect_simple(TokenKind::RParen, "expected ')' after arguments")?;
                let span = Span::merge(expr.span(), end.span);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
                continue;
            }

            if self.match_simple(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                let end = self.expect_simple(TokenKind::RBracket, "expected ']' after index")?;
                let span = Span::merge(expr.span(), end.span);
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
                continue;
            }

            if self.match_simple(TokenKind::Dot) {
                if let Some((index, index_span)) = self.match_int() {
                    if index < 0 {
                        return Err(SaftError::with_span(
                            "tuple index must be non-negative",
                            index_span,
                        ));
                    }
                    let span = Span::merge(expr.span(), index_span);
                    expr = Expr::TupleIndex {
                        target: Box::new(expr),
                        index: index as usize,
                        span,
                    };
                    continue;
                }

                if let Some((name, name_span)) = self.match_ident() {
                    let span = Span::merge(expr.span(), name_span);
                    expr = Expr::Member {
                        target: Box::new(expr),
                        name,
                        span,
                    };
                    continue;
                }

                return Err(SaftError::with_span(
                    "expected field name or tuple index after '.'",
                    self.current().span,
                ));
            }

            break;
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> SaftResult<Expr> {
        match &self.current().kind {
            TokenKind::Int(value) => {
                let span = self.current().span;
                let value = *value;
                self.advance();
                Ok(Expr::Int(value, span))
            }
            TokenKind::Float(value) => {
                let span = self.current().span;
                let value = *value;
                self.advance();
                Ok(Expr::Float(value, span))
            }
            TokenKind::String(value) => {
                let span = self.current().span;
                let value = value.clone();
                self.advance();
                Ok(Expr::Str(value, span))
            }
            TokenKind::True => {
                let span = self.current().span;
                self.advance();
                Ok(Expr::Bool(true, span))
            }
            TokenKind::False => {
                let span = self.current().span;
                self.advance();
                Ok(Expr::Bool(false, span))
            }
            TokenKind::Nil => {
                let span = self.current().span;
                self.advance();
                Ok(Expr::Nil(span))
            }
            TokenKind::Ident(name) => {
                let span = self.current().span;
                let name = name.clone();
                self.advance();
                Ok(Expr::Var(name, span))
            }
            TokenKind::LBracket => self.parse_list_lit(),
            TokenKind::LBrace => self.parse_object_lit(),
            TokenKind::LParen => self.parse_group_or_tuple(),
            TokenKind::Prompt(_) => self.parse_prompt_expr(),
            _ => Err(SaftError::with_span(
                "expected expression",
                self.current().span,
            )),
        }
    }

    fn parse_prompt_expr(&mut self) -> SaftResult<Expr> {
        let token = self.advance();
        let span = token.span;
        let TokenKind::Prompt(raw) = token.kind else {
            return Err(SaftError::with_span(
                "internal parser error: expected prompt token",
                span,
            ));
        };

        let parts = self.parse_prompt_parts(&raw, span)?;
        Ok(Expr::Prompt(PromptExpr { parts, span }))
    }

    fn parse_prompt_parts(&self, raw: &str, span: Span) -> SaftResult<Vec<PromptPart>> {
        let mut parts = Vec::new();
        let mut text_start = 0usize;
        let bytes = raw.as_bytes();
        let mut idx = 0usize;

        while idx < bytes.len() {
            if bytes[idx] != b'{' {
                idx += 1;
                continue;
            }

            if text_start < idx {
                parts.push(PromptPart::Text(raw[text_start..idx].to_string()));
            }

            let close_idx = Self::find_prompt_interpolation_end(raw, idx)
                .ok_or_else(|| SaftError::with_span("unterminated prompt interpolation", span))?;

            let expr_source = raw[idx + 1..close_idx].trim();
            if expr_source.is_empty() {
                return Err(SaftError::with_span(
                    "empty prompt interpolation is not allowed",
                    span,
                ));
            }

            let expr = Self::parse_embedded_expr(expr_source, span)?;
            parts.push(PromptPart::Interpolation(expr));

            idx = close_idx + 1;
            text_start = idx;
        }

        if text_start < raw.len() {
            parts.push(PromptPart::Text(raw[text_start..].to_string()));
        }

        if parts.is_empty() {
            parts.push(PromptPart::Text(String::new()));
        }

        Ok(parts)
    }

    fn find_prompt_interpolation_end(raw: &str, open_idx: usize) -> Option<usize> {
        let bytes = raw.as_bytes();
        let mut idx = open_idx + 1;
        let mut brace_depth = 1usize;
        let mut in_string = false;
        let mut escaped = false;

        while idx < bytes.len() {
            let byte = bytes[idx];

            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
                idx += 1;
                continue;
            }

            match byte {
                b'"' => in_string = true,
                b'{' => brace_depth += 1,
                b'}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }

            idx += 1;
        }

        None
    }

    fn parse_embedded_expr(source: &str, prompt_span: Span) -> SaftResult<Expr> {
        let mut expr_source = source.to_string();
        expr_source.push('\n');

        let tokens = lexer::lex(&expr_source).map_err(|err| {
            SaftError::with_span(
                format!("invalid prompt interpolation: {}", err.message),
                prompt_span,
            )
        })?;

        let mut parser = Parser::new(tokens);
        parser.consume_newlines();
        let expr = parser.parse_expr().map_err(|err| {
            SaftError::with_span(
                format!("invalid prompt interpolation: {}", err.message),
                prompt_span,
            )
        })?;
        parser.consume_newlines();

        if !parser.is_eof() {
            return Err(SaftError::with_span(
                "invalid prompt interpolation: trailing tokens",
                prompt_span,
            ));
        }

        Ok(expr)
    }

    fn parse_list_lit(&mut self) -> SaftResult<Expr> {
        let start = self
            .expect_simple(TokenKind::LBracket, "expected '['")?
            .span;
        let mut items = Vec::new();
        if !self.check_simple(&TokenKind::RBracket) {
            loop {
                items.push(self.parse_expr()?);
                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
            }
        }

        let end = self.expect_simple(TokenKind::RBracket, "expected ']' after list")?;
        Ok(Expr::List(items, Span::merge(start, end.span)))
    }

    fn parse_group_or_tuple(&mut self) -> SaftResult<Expr> {
        let start = self.expect_simple(TokenKind::LParen, "expected '('")?.span;
        let first = self.parse_expr()?;

        if self.match_simple(TokenKind::Comma) {
            let mut items = vec![first];
            loop {
                items.push(self.parse_expr()?);
                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
            }
            let end = self.expect_simple(TokenKind::RParen, "expected ')' after tuple")?;
            return Ok(Expr::Tuple(items, Span::merge(start, end.span)));
        }

        self.expect_simple(TokenKind::RParen, "expected ')' after expression")?;
        Ok(first)
    }

    fn parse_object_lit(&mut self) -> SaftResult<Expr> {
        let start = self.expect_simple(TokenKind::LBrace, "expected '{'")?.span;
        self.consume_soft_breaks();
        let mut fields = Vec::new();

        if !self.check_simple(&TokenKind::RBrace) {
            loop {
                self.consume_soft_breaks();
                let (name, _) = self.expect_ident("expected object field name")?;
                self.expect_simple(TokenKind::Colon, "expected ':' after object field name")?;
                let value = self.parse_expr()?;
                fields.push((name, value));
                self.consume_soft_breaks();

                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
                self.consume_soft_breaks();
            }
        }

        self.consume_soft_breaks();
        let end = self.expect_simple(TokenKind::RBrace, "expected '}' after object")?;
        Ok(Expr::Object(fields, Span::merge(start, end.span)))
    }

    fn parse_schema_expr(&mut self) -> SaftResult<SchemaExpr> {
        self.parse_union_schema()
    }

    fn parse_union_schema(&mut self) -> SaftResult<SchemaExpr> {
        let mut variants = vec![self.parse_schema_primary()?];

        while self.match_simple(TokenKind::Pipe) {
            variants.push(self.parse_schema_primary()?);
        }

        let mut schema = if variants.len() == 1 {
            variants.pop().expect("variants has one item")
        } else {
            SchemaExpr::Union(variants)
        };

        if self.match_simple(TokenKind::Question) {
            schema = SchemaExpr::Optional(Box::new(schema));
        }

        Ok(schema)
    }

    fn parse_schema_primary(&mut self) -> SaftResult<SchemaExpr> {
        if let Some((name, span)) = self.match_ident() {
            let schema = match name.as_str() {
                "any" => SchemaExpr::Any,
                "int" => SchemaExpr::Int,
                "float" => SchemaExpr::Float,
                "bool" => SchemaExpr::Bool,
                "string" => SchemaExpr::String,
                _ => {
                    return Err(SaftError::with_span(
                        format!("unknown schema type '{name}'"),
                        span,
                    ));
                }
            };
            return Ok(schema);
        }

        if self.match_simple(TokenKind::LBracket) {
            let inner = self.parse_schema_expr()?;
            self.expect_simple(TokenKind::RBracket, "expected ']' in list schema")?;
            return Ok(SchemaExpr::List(Box::new(inner)));
        }

        if self.match_simple(TokenKind::LParen) {
            let first = self.parse_schema_expr()?;
            if self.match_simple(TokenKind::Comma) {
                let mut items = vec![first];
                loop {
                    items.push(self.parse_schema_expr()?);
                    if !self.match_simple(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect_simple(TokenKind::RParen, "expected ')' after tuple schema")?;
                return Ok(SchemaExpr::Tuple(items));
            }
            self.expect_simple(TokenKind::RParen, "expected ')' after grouped schema")?;
            return Ok(first);
        }

        if self.match_simple(TokenKind::LBrace) {
            self.consume_soft_breaks();
            let mut fields = Vec::new();
            if self.check_simple(&TokenKind::RBrace) {
                return Err(SaftError::with_span(
                    "object schema requires at least one field",
                    self.current().span,
                ));
            }

            loop {
                self.consume_soft_breaks();
                let (name, _) = self.expect_ident("expected field name in object schema")?;
                self.expect_simple(TokenKind::Colon, "expected ':' after field name")?;
                let schema = self.parse_schema_expr()?;
                fields.push(SchemaField { name, schema });
                self.consume_soft_breaks();
                if !self.match_simple(TokenKind::Comma) {
                    break;
                }
                self.consume_soft_breaks();
            }

            self.consume_soft_breaks();
            self.expect_simple(TokenKind::RBrace, "expected '}' after object schema")?;
            return Ok(SchemaExpr::Object(fields));
        }

        Err(SaftError::with_span(
            "expected schema expression",
            self.current().span,
        ))
    }

    fn is_assign_stmt_start(&self) -> bool {
        matches!(self.current().kind, TokenKind::Ident(_))
            && matches!(self.peek(1).kind, TokenKind::Eq | TokenKind::Colon)
    }

    fn consume_newlines(&mut self) {
        while self.match_simple(TokenKind::Newline) {}
    }

    fn consume_soft_breaks(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.advance();
        }
    }

    fn expect_simple(&mut self, expected: TokenKind, message: &str) -> SaftResult<Token> {
        if self.check_simple(&expected) {
            Ok(self.advance())
        } else {
            Err(SaftError::with_span(message, self.current().span))
        }
    }

    fn check_simple(&self, expected: &TokenKind) -> bool {
        self.current().kind.same_variant(expected)
    }

    fn match_simple(&mut self, expected: TokenKind) -> bool {
        if self.check_simple(&expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self, message: &str) -> SaftResult<(String, Span)> {
        self.match_ident()
            .ok_or_else(|| SaftError::with_span(message, self.current().span))
    }

    fn match_ident(&mut self) -> Option<(String, Span)> {
        if let TokenKind::Ident(name) = &self.current().kind {
            let span = self.current().span;
            let name = name.clone();
            self.advance();
            Some((name, span))
        } else {
            None
        }
    }

    fn match_int(&mut self) -> Option<(i64, Span)> {
        if let TokenKind::Int(value) = self.current().kind {
            let span = self.current().span;
            self.advance();
            Some((value, span))
        } else {
            None
        }
    }

    fn is_eof(&self) -> bool {
        self.check_simple(&TokenKind::Eof)
    }

    fn current(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("token stream is non-empty"))
    }

    fn peek(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.pos + offset)
            .unwrap_or_else(|| self.tokens.last().expect("token stream is non-empty"))
    }

    fn advance(&mut self) -> Token {
        let tok = self.current().clone();
        if !self.is_eof() {
            self.pos += 1;
        }
        tok
    }

    fn previous(&self) -> &Token {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .unwrap_or_else(|| self.tokens.first().expect("token stream is non-empty"))
    }
}
