use crate::error::{SaftError, SaftResult, Span};
use crate::token::{Token, TokenKind};

pub fn lex(source: &str) -> SaftResult<Vec<Token>> {
    Lexer::new(source).lex()
}

struct Lexer<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    indent_stack: Vec<usize>,
    in_prompt_block: bool,
    prompt_start_span: Option<Span>,
    prompt_buffer: String,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            tokens: Vec::new(),
            indent_stack: vec![0],
            in_prompt_block: false,
            prompt_start_span: None,
            prompt_buffer: String::new(),
        }
    }

    fn lex(mut self) -> SaftResult<Vec<Token>> {
        let mut offset = 0usize;
        let mut line_no = 1usize;

        for raw_line in self.source.split_inclusive('\n') {
            self.lex_line(raw_line, line_no, offset)?;
            offset += raw_line.len();
            line_no += 1;
        }

        let eof_line = line_no.saturating_sub(1).max(1);

        if self.in_prompt_block {
            let span = Span::new(offset, offset, eof_line, 1);
            return Err(SaftError::with_span("unterminated prompt block", span));
        }

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            let span = Span::new(offset, offset, eof_line, 1);
            self.tokens.push(Token::new(TokenKind::Dedent, span));
        }

        let eof_span = Span::new(offset, offset, eof_line, 1);
        self.tokens.push(Token::new(TokenKind::Eof, eof_span));
        Ok(self.tokens)
    }

    fn lex_line(&mut self, raw_line: &str, line_no: usize, line_start: usize) -> SaftResult<()> {
        let has_newline = raw_line.ends_with('\n');
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let bytes = line.as_bytes();

        if self.in_prompt_block {
            return self.lex_prompt_line(line, line_no, line_start, has_newline);
        }

        let mut idx = 0usize;
        let mut indent = 0usize;
        while idx < bytes.len() {
            match bytes[idx] {
                b' ' => {
                    indent += 1;
                    idx += 1;
                }
                b'\t' => {
                    let span = Span::new(line_start + idx, line_start + idx + 1, line_no, idx + 1);
                    return Err(SaftError::with_span(
                        "tabs are not supported for indentation; use spaces",
                        span,
                    ));
                }
                _ => break,
            }
        }

        let rest = &line[idx..];
        if rest.trim().is_empty() || rest.trim_start().starts_with("//") {
            return Ok(());
        }

        self.handle_indentation(indent, line_no, line_start)?;

        while idx < bytes.len() {
            if bytes[idx] == b' ' {
                idx += 1;
                continue;
            }

            if line[idx..].starts_with("//") {
                break;
            }

            if bytes[idx] == b'$' {
                return self.start_prompt(line, line_no, line_start, idx, has_newline);
            }

            let token_start = idx;
            let start_col = token_start + 1;
            let kind = match bytes[idx] {
                b'(' => {
                    idx += 1;
                    TokenKind::LParen
                }
                b')' => {
                    idx += 1;
                    TokenKind::RParen
                }
                b'[' => {
                    idx += 1;
                    TokenKind::LBracket
                }
                b']' => {
                    idx += 1;
                    TokenKind::RBracket
                }
                b'{' => {
                    idx += 1;
                    TokenKind::LBrace
                }
                b'}' => {
                    idx += 1;
                    TokenKind::RBrace
                }
                b',' => {
                    idx += 1;
                    TokenKind::Comma
                }
                b':' => {
                    idx += 1;
                    TokenKind::Colon
                }
                b'.' => {
                    idx += 1;
                    TokenKind::Dot
                }
                b'+' => {
                    idx += 1;
                    TokenKind::Plus
                }
                b'-' => {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'>' {
                        idx += 2;
                        TokenKind::Arrow
                    } else {
                        idx += 1;
                        TokenKind::Minus
                    }
                }
                b'*' => {
                    idx += 1;
                    TokenKind::Star
                }
                b'/' => {
                    idx += 1;
                    TokenKind::Slash
                }
                b'%' => {
                    idx += 1;
                    TokenKind::Percent
                }
                b'=' => {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'=' {
                        idx += 2;
                        TokenKind::EqEq
                    } else {
                        idx += 1;
                        TokenKind::Eq
                    }
                }
                b'!' => {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'=' {
                        idx += 2;
                        TokenKind::BangEq
                    } else {
                        let span = Span::new(
                            line_start + token_start,
                            line_start + token_start + 1,
                            line_no,
                            start_col,
                        );
                        return Err(SaftError::with_span(
                            "unexpected '!' (did you mean '!=')",
                            span,
                        ));
                    }
                }
                b'<' => {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'=' {
                        idx += 2;
                        TokenKind::LtEq
                    } else {
                        idx += 1;
                        TokenKind::Lt
                    }
                }
                b'>' => {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'=' {
                        idx += 2;
                        TokenKind::GtEq
                    } else {
                        idx += 1;
                        TokenKind::Gt
                    }
                }
                b'|' => {
                    idx += 1;
                    TokenKind::Pipe
                }
                b'?' => {
                    idx += 1;
                    TokenKind::Question
                }
                b'"' => {
                    idx += 1;
                    let mut out = String::new();
                    let mut closed = false;

                    while idx < bytes.len() {
                        match bytes[idx] {
                            b'"' => {
                                idx += 1;
                                closed = true;
                                break;
                            }
                            b'\\' => {
                                idx += 1;
                                if idx >= bytes.len() {
                                    break;
                                }

                                let escaped = match bytes[idx] {
                                    b'n' => '\n',
                                    b't' => '\t',
                                    b'r' => '\r',
                                    b'"' => '"',
                                    b'\\' => '\\',
                                    other => {
                                        let span = Span::new(
                                            line_start + idx,
                                            line_start + idx + 1,
                                            line_no,
                                            idx + 1,
                                        );
                                        return Err(SaftError::with_span(
                                            format!(
                                                "unsupported string escape: \\\\x{:02x}",
                                                other
                                            ),
                                            span,
                                        ));
                                    }
                                };
                                out.push(escaped);
                                idx += 1;
                            }
                            byte => {
                                out.push(char::from(byte));
                                idx += 1;
                            }
                        }
                    }

                    if !closed {
                        let span = Span::new(
                            line_start + token_start,
                            line_start + idx,
                            line_no,
                            start_col,
                        );
                        return Err(SaftError::with_span("unterminated string literal", span));
                    }

                    TokenKind::String(out)
                }
                byte if is_ident_start(byte) => {
                    idx += 1;
                    while idx < bytes.len() && is_ident_continue(bytes[idx]) {
                        idx += 1;
                    }

                    let text = &line[token_start..idx];
                    match text {
                        "f" => TokenKind::F,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "for" => TokenKind::For,
                        "in" => TokenKind::In,
                        "ret" => TokenKind::Ret,
                        "assert" => TokenKind::Assert,
                        "and" => TokenKind::And,
                        "or" => TokenKind::Or,
                        "not" => TokenKind::Not,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "nil" => TokenKind::Nil,
                        _ => TokenKind::Ident(text.to_string()),
                    }
                }
                byte if byte.is_ascii_digit() => {
                    idx += 1;
                    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                        idx += 1;
                    }

                    let mut is_float = false;
                    if idx + 1 < bytes.len()
                        && bytes[idx] == b'.'
                        && bytes[idx + 1].is_ascii_digit()
                    {
                        is_float = true;
                        idx += 1;
                        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                            idx += 1;
                        }
                    }

                    let text = &line[token_start..idx];
                    if is_float {
                        let value = text.parse::<f64>().map_err(|_| {
                            let span = Span::new(
                                line_start + token_start,
                                line_start + idx,
                                line_no,
                                start_col,
                            );
                            SaftError::with_span("invalid float literal", span)
                        })?;
                        TokenKind::Float(value)
                    } else {
                        let value = text.parse::<i64>().map_err(|_| {
                            let span = Span::new(
                                line_start + token_start,
                                line_start + idx,
                                line_no,
                                start_col,
                            );
                            SaftError::with_span("invalid integer literal", span)
                        })?;
                        TokenKind::Int(value)
                    }
                }
                other => {
                    let span = Span::new(
                        line_start + token_start,
                        line_start + token_start + 1,
                        line_no,
                        start_col,
                    );
                    return Err(SaftError::with_span(
                        format!("unexpected character '{}'", char::from(other)),
                        span,
                    ));
                }
            };

            let span = Span::new(
                line_start + token_start,
                line_start + idx,
                line_no,
                start_col,
            );
            self.tokens.push(Token::new(kind, span));
        }

        let nl_col = line.len() + 1;
        let nl_span = Span::new(
            line_start + line.len(),
            line_start + line.len(),
            line_no,
            nl_col,
        );
        self.tokens.push(Token::new(TokenKind::Newline, nl_span));
        Ok(())
    }

    fn start_prompt(
        &mut self,
        line: &str,
        line_no: usize,
        line_start: usize,
        dollar_idx: usize,
        has_newline: bool,
    ) -> SaftResult<()> {
        let start_span = Span::new(
            line_start + dollar_idx,
            line_start + dollar_idx + 1,
            line_no,
            dollar_idx + 1,
        );

        let after_open = dollar_idx + 1;
        if let Some(rel_close_idx) = line[after_open..].find('$') {
            let close_idx = after_open + rel_close_idx;
            let content = line[after_open..close_idx].to_string();
            let close_span = Span::new(
                line_start + close_idx,
                line_start + close_idx + 1,
                line_no,
                close_idx + 1,
            );

            self.tokens.push(Token::new(
                TokenKind::Prompt(content),
                Span::merge(start_span, close_span),
            ));

            let rest = &line[close_idx + 1..];
            if !rest.trim().is_empty() && !rest.trim_start().starts_with("//") {
                return Err(SaftError::with_span(
                    "unexpected text after closing '$'",
                    close_span,
                ));
            }

            let nl_col = line.len() + 1;
            let nl_span = Span::new(
                line_start + line.len(),
                line_start + line.len(),
                line_no,
                nl_col,
            );
            self.tokens.push(Token::new(TokenKind::Newline, nl_span));
            return Ok(());
        }

        self.in_prompt_block = true;
        self.prompt_start_span = Some(start_span);
        self.prompt_buffer.clear();
        self.prompt_buffer.push_str(&line[after_open..]);
        if has_newline {
            self.prompt_buffer.push('\n');
        }

        Ok(())
    }

    fn lex_prompt_line(
        &mut self,
        line: &str,
        line_no: usize,
        line_start: usize,
        has_newline: bool,
    ) -> SaftResult<()> {
        if let Some(close_idx) = line.find('$') {
            self.prompt_buffer.push_str(&line[..close_idx]);

            let start_span = self.prompt_start_span.take().ok_or_else(|| {
                SaftError::new("internal lexer state error: prompt start span missing")
            })?;

            let close_span = Span::new(
                line_start + close_idx,
                line_start + close_idx + 1,
                line_no,
                close_idx + 1,
            );

            let content = std::mem::take(&mut self.prompt_buffer);
            self.tokens.push(Token::new(
                TokenKind::Prompt(content),
                Span::merge(start_span, close_span),
            ));
            self.in_prompt_block = false;

            let rest = &line[close_idx + 1..];
            if !rest.trim().is_empty() && !rest.trim_start().starts_with("//") {
                return Err(SaftError::with_span(
                    "unexpected text after closing '$'",
                    close_span,
                ));
            }

            let nl_col = line.len() + 1;
            let nl_span = Span::new(
                line_start + line.len(),
                line_start + line.len(),
                line_no,
                nl_col,
            );
            self.tokens.push(Token::new(TokenKind::Newline, nl_span));
            return Ok(());
        }

        self.prompt_buffer.push_str(line);
        if has_newline {
            self.prompt_buffer.push('\n');
        }

        Ok(())
    }

    fn handle_indentation(
        &mut self,
        indent: usize,
        line_no: usize,
        line_start: usize,
    ) -> SaftResult<()> {
        let current = *self.indent_stack.last().unwrap_or(&0);

        if indent > current {
            self.indent_stack.push(indent);
            let span = Span::new(line_start, line_start + indent, line_no, 1);
            self.tokens.push(Token::new(TokenKind::Indent, span));
            return Ok(());
        }

        if indent < current {
            while self.indent_stack.len() > 1 {
                let top = *self.indent_stack.last().unwrap_or(&0);
                if indent >= top {
                    break;
                }
                self.indent_stack.pop();
                let span = Span::new(line_start, line_start + indent, line_no, 1);
                self.tokens.push(Token::new(TokenKind::Dedent, span));
            }

            let top = *self.indent_stack.last().unwrap_or(&0);
            if indent != top {
                let span = Span::new(line_start, line_start + indent, line_no, 1);
                return Err(SaftError::with_span("inconsistent indentation level", span));
            }
        }

        Ok(())
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}
