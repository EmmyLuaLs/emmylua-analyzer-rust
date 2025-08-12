use emmylua_parser::{LexerState, Reader, SourceRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimTokenKind {
    TkEof,
    TkEndOfLine,
    TkWhitespace,
    TkComment,
    TkString,
    TkNumber,
    TkKeyword,
    TkFunction,
    TkVariable,
    TkOperator,
    TkUnknown,
}

#[derive(Debug)]
pub struct VimTokenData {
    pub kind: VimTokenKind,
    pub range: SourceRange,
}

impl VimTokenData {
    pub fn new(kind: VimTokenKind, range: SourceRange) -> Self {
        Self { kind, range }
    }
}

#[derive(Debug)]
pub struct VimscriptLexer<'a> {
    reader: Reader<'a>,
    state: LexerState,
}

impl<'a> VimscriptLexer<'a> {
    pub fn new_with_state(reader: Reader<'a>, state: LexerState) -> Self {
        VimscriptLexer { reader, state }
    }

    pub fn tokenize(&mut self) -> Vec<VimTokenData> {
        let mut tokens = vec![];

        while !self.reader.is_eof() {
            let kind = match self.state {
                LexerState::Normal => self.lex(),
                LexerState::String(quote) => self.lex_string(quote),
                _ => VimTokenKind::TkUnknown,
            };

            if kind == VimTokenKind::TkEof {
                break;
            }

            tokens.push(VimTokenData::new(kind, self.reader.current_range()));
        }

        tokens
    }

    pub fn get_state(&self) -> LexerState {
        self.state
    }

    fn lex(&mut self) -> VimTokenKind {
        self.reader.reset_buff();

        match self.reader.current_char() {
            '\n' | '\r' => self.lex_new_line(),
            ' ' | '\t' => self.lex_white_space(),
            '"' => self.lex_comment(),
            '\'' => {
                let quote = self.reader.current_char();
                self.reader.bump();
                self.state = LexerState::String(quote);
                self.lex_string(quote)
            }
            '0'..='9' => self.lex_number(),
            'a'..='z' | 'A'..='Z' | '_' => self.lex_name(),
            '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '('
            | ')' | '[' | ']' | '{' | '}' | ',' | '.' | ':' | ';' => {
                self.reader.bump();
                VimTokenKind::TkOperator
            }
            _ if self.reader.is_eof() => VimTokenKind::TkEof,
            _ => {
                self.reader.bump();
                VimTokenKind::TkUnknown
            }
        }
    }

    fn lex_new_line(&mut self) -> VimTokenKind {
        if self.reader.current_char() == '\r' {
            self.reader.bump();
        }
        if self.reader.current_char() == '\n' {
            self.reader.bump();
        }
        VimTokenKind::TkEndOfLine
    }

    fn lex_white_space(&mut self) -> VimTokenKind {
        self.reader.eat_while(|c| c == ' ' || c == '\t');
        VimTokenKind::TkWhitespace
    }

    fn lex_comment(&mut self) -> VimTokenKind {
        if self.reader.current_char() == '"' {
            self.reader.bump();
            self.reader.eat_while(|ch| ch != '\n' && ch != '\r');
            VimTokenKind::TkComment
        } else {
            self.reader.bump();
            VimTokenKind::TkUnknown
        }
    }

    fn lex_string(&mut self, quote: char) -> VimTokenKind {
        while !self.reader.is_eof() {
            let ch = self.reader.current_char();
            if ch == quote || ch == '\n' || ch == '\r' {
                break;
            }

            if ch == '\\' {
                self.reader.bump();
                if !self.reader.is_eof() {
                    self.reader.bump();
                }
            } else {
                self.reader.bump();
            }
        }

        if self.reader.current_char() == quote {
            self.reader.bump();
            self.state = LexerState::Normal;
        }

        VimTokenKind::TkString
    }

    fn lex_number(&mut self) -> VimTokenKind {
        self.reader.eat_while(|c| c.is_ascii_digit());

        if self.reader.current_char() == '.' && self.reader.next_char().is_ascii_digit() {
            self.reader.bump();
            self.reader.eat_while(|c| c.is_ascii_digit());
        }

        VimTokenKind::TkNumber
    }

    fn lex_name(&mut self) -> VimTokenKind {
        self.reader
            .eat_while(|c| c.is_alphanumeric() || c == '_' || c == '#');
        let name = self.reader.current_text();
        self.name_to_kind(name)
    }

    fn name_to_kind(&self, name: &str) -> VimTokenKind {
        match name {
            // Vim keywords
            "if" | "then" | "else" | "elseif" | "endif" | "while" | "endwhile" | "for"
            | "endfor" | "in" | "function" | "endfunction" | "return" | "try" | "catch"
            | "finally" | "endtry" | "throw" | "let" | "const" | "unlet" | "echo" | "echom"
            | "echon" | "echoerr" | "set" | "setlocal" | "setglobal" | "augroup" | "autocmd"
            | "command" | "noremap" | "nmap" | "nnoremap" | "imap" | "inoremap" | "vmap"
            | "vnoremap" | "cmap" | "cnoremap" | "source" | "runtime" | "execute" | "eval"
            | "exists" | "type" | "string" | "printf" | "call" | "apply" | "filter" | "split"
            | "join" | "substitute" | "match" | "expand" | "glob" | "resolve" | "fnamemodify"
            | "bufexists" | "bufname" | "bufnr" | "getline" | "setline" | "winheight"
            | "winwidth" | "wincol" | "winline" | "tabpagenr" | "tabpagewinnr" | "gettabvar"
            | "settabvar" => VimTokenKind::TkKeyword,

            // Common Vim functions
            _ if name.starts_with("g:")
                || name.starts_with("l:")
                || name.starts_with("s:")
                || name.starts_with("a:")
                || name.starts_with("w:")
                || name.starts_with("t:")
                || name.starts_with("b:") =>
            {
                VimTokenKind::TkVariable
            }

            _ if self.is_function_name(name) => VimTokenKind::TkFunction,

            _ => VimTokenKind::TkVariable,
        }
    }

    fn is_function_name(&self, name: &str) -> bool {
        // Check if it's a common vim function or ends with ()
        matches!(
            name,
            "abs"
                | "acos"
                | "add"
                | "and"
                | "append"
                | "argc"
                | "argv"
                | "asin"
                | "atan"
                | "browse"
                | "bufexists"
                | "buflisted"
                | "bufloaded"
                | "bufname"
                | "bufnr"
                | "bufwinnr"
                | "ceil"
                | "changenr"
                | "char2nr"
                | "cindent"
                | "clearmatches"
                | "col"
                | "complete"
                | "complete_add"
                | "complete_check"
                | "confirm"
                | "copy"
                | "cos"
                | "count"
                | "cscope_connection"
                | "cursor"
                | "deepcopy"
                | "delete"
                | "did_filetype"
                | "diff_filler"
                | "diff_hlID"
                | "empty"
                | "escape"
                | "eval"
                | "eventhandler"
                | "executable"
                | "exists"
                | "exp"
                | "expand"
                | "extend"
                | "feedkeys"
                | "file_readable"
                | "filereadable"
                | "filewritable"
                | "filter"
                | "finddir"
                | "findfile"
                | "float2nr"
                | "floor"
                | "fmod"
                | "fnameescape"
                | "fnamemodify"
                | "foldclosed"
                | "foldclosedend"
                | "foldlevel"
                | "foldtext"
                | "foldtextresult"
                | "foreground"
                | "function"
                | "garbagecollect"
                | "get"
                | "getbufline"
                | "getbufvar"
                | "getchar"
                | "getcharmod"
                | "getcmdline"
                | "getcmdpos"
                | "getcmdtype"
                | "getcwd"
                | "getfperm"
                | "getfsize"
                | "getftime"
                | "getftype"
                | "getline"
                | "getloclist"
                | "getmatches"
                | "getpid"
                | "getpos"
                | "getqflist"
                | "getreg"
                | "getregtype"
                | "gettabvar"
                | "gettabwinvar"
                | "getwinposx"
                | "getwinposy"
                | "getwinvar"
                | "glob"
                | "globpath"
                | "has"
                | "has_key"
                | "haslocaldir"
                | "hasmapto"
                | "histadd"
                | "histdel"
                | "histget"
                | "hlexists"
                | "hlID"
                | "hostname"
                | "iconv"
                | "indent"
                | "index"
                | "input"
                | "inputdialog"
                | "inputlist"
                | "inputrestore"
                | "inputsave"
                | "inputsecret"
                | "insert"
                | "invert"
                | "isdirectory"
                | "islocked"
                | "join"
                | "keys"
                | "len"
                | "libcall"
                | "libcallnr"
                | "line"
                | "line2byte"
                | "lispindent"
                | "localtime"
                | "log10"
                | "luaeval"
                | "map"
                | "maparg"
                | "mapcheck"
                | "match"
                | "matchadd"
                | "matcharg"
                | "matchdelete"
                | "matchend"
                | "matchlist"
                | "matchstr"
                | "max"
                | "min"
                | "mkdir"
                | "mode"
                | "nextnonblank"
                | "nr2byte"
                | "nr2char"
                | "nr2float"
                | "or"
                | "pathshorten"
                | "pow"
                | "prevnonblank"
                | "printf"
                | "pumvisible"
                | "range"
                | "readfile"
                | "reltime"
                | "reltimestr"
                | "remote_expr"
                | "remote_foreground"
                | "remote_peek"
                | "remote_read"
                | "remote_send"
                | "remove"
                | "rename"
                | "repeat"
                | "resolve"
                | "reverse"
                | "round"
                | "search"
                | "searchdecl"
                | "searchpair"
                | "searchpairpos"
                | "searchpos"
                | "server2client"
                | "serverlist"
                | "setbufvar"
                | "setcmdpos"
                | "setline"
                | "setloclist"
                | "setmatches"
                | "setpos"
                | "setqflist"
                | "setreg"
                | "settabvar"
                | "settabwinvar"
                | "setwinvar"
                | "shellescape"
                | "shiftwidth"
                | "simplify"
                | "sin"
                | "sinh"
                | "sort"
                | "soundfold"
                | "spellbadword"
                | "spellsuggest"
                | "split"
                | "sqrt"
                | "str2float"
                | "str2nr"
                | "strchars"
                | "strdisplaywidth"
                | "strftime"
                | "stridx"
                | "string"
                | "strlen"
                | "strpart"
                | "strridx"
                | "strtrans"
                | "strwidth"
                | "submatch"
                | "substitute"
                | "synID"
                | "synIDattr"
                | "synIDtrans"
                | "synstack"
                | "system"
                | "tabpagebuflist"
                | "tabpagenr"
                | "tabpagewinnr"
                | "tagfiles"
                | "taglist"
                | "tan"
                | "tanh"
                | "tempname"
                | "tolower"
                | "toupper"
                | "tr"
                | "trunc"
                | "type"
                | "undofile"
                | "undotree"
                | "uniq"
                | "values"
                | "virtcol"
                | "visualmode"
                | "winbufnr"
                | "wincol"
                | "winheight"
                | "winline"
                | "winnr"
                | "winrestcmd"
                | "winrestview"
                | "winsaveview"
                | "winwidth"
                | "writefile"
                | "xor"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emmylua_parser::Reader;
    use googletest::prelude::*;

    #[gtest]
    fn test_vim_lexer_basic() {
        let code = r#"
" This is a comment
function! TestFunction()
    let g:my_var = "hello world"
    echo "Hello from Vim!"
    if exists('g:my_var')
        echo g:my_var
    endif
    return 42
endfunction
"#;

        let reader = Reader::new(code);
        let mut lexer = VimscriptLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        // Count different token types
        let mut keyword_count = 0;
        let mut string_count = 0;
        let mut comment_count = 0;
        let mut number_count = 0;

        for token in &tokens {
            match token.kind {
                VimTokenKind::TkKeyword => keyword_count += 1,
                VimTokenKind::TkString => string_count += 1,
                VimTokenKind::TkComment => comment_count += 1,
                VimTokenKind::TkNumber => number_count += 1,
                _ => {}
            }
        }

        expect_gt!(keyword_count, 0, "Should find keywords");
        expect_gt!(string_count, 0, "Should find strings");
        expect_gt!(comment_count, 0, "Should find comments");
        expect_gt!(number_count, 0, "Should find numbers");

        println!(
            "Found {} keywords, {} strings, {} comments, {} numbers",
            keyword_count, string_count, comment_count, number_count
        );
    }

    #[gtest]
    fn test_vim_lexer_keywords() {
        let code = "function! if else endif let echo return";

        let reader = Reader::new(code);
        let mut lexer = VimscriptLexer::new_with_state(reader, LexerState::Normal);
        let tokens = lexer.tokenize();

        let keywords: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == VimTokenKind::TkKeyword)
            .collect();

        expect_ge!(keywords.len(), 5, "Should find multiple keywords");
    }
}
