//! LaTeX, as Markdown export writes it, back into an Office Math fragment.
//!
//! The fragment is what a saved document keeps. Reading that document runs the
//! Office Math converter, so a formula comes back in that converter's spelling.

pub(crate) fn omml_from_latex(latex: &str, display: bool) -> String {
    let latex = latex.trim();
    let mut parser = Parser { src: latex, i: 0 };
    let (align, body) = parser.display_body();
    let math = format!("<m:oMath>{body}</m:oMath>");
    if !display {
        return math;
    }
    let props = match align {
        Align::Left => "<m:oMathParaPr><m:jc m:val=\"left\"/></m:oMathParaPr>",
        Align::Right => "<m:oMathParaPr><m:jc m:val=\"right\"/></m:oMathParaPr>",
        Align::Center => "",
    };
    format!("<m:oMathPara>{props}{math}</m:oMathPara>")
}

#[derive(Clone, Copy)]
enum Stop {
    End,
    Brace,
    Right,
    Cell,
}

#[derive(Clone, Copy)]
enum Align {
    Center,
    Left,
    Right,
}

struct Parser<'a> {
    src: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.i..]
    }

    fn eat(&mut self, prefix: &str) -> bool {
        if self.rest().starts_with(prefix) {
            self.i += prefix.len();
            true
        } else {
            false
        }
    }

    fn display_body(&mut self) -> (Align, String) {
        if let Some(inner) = self.rest().strip_prefix("\\begin{aligned}") {
            if let Some(end) = inner.strip_suffix("\\end{aligned}") {
                if !end.contains("\\\\") && end.matches('&').count() == 1 {
                    if let Some(body) = end.strip_prefix('&') {
                        if !body.contains('&') {
                            self.i = self.src.len();
                            return (Align::Left, self.parse_source(body));
                        }
                    }
                    if let Some(body) = end.strip_suffix('&') {
                        if !body.contains('&') {
                            self.i = self.src.len();
                            return (Align::Right, self.parse_source(body));
                        }
                    }
                }
            }
        }
        (Align::Center, self.parse_seq(Stop::End))
    }

    fn parse_source(&self, source: &str) -> String {
        Parser { src: source, i: 0 }.parse_seq(Stop::End)
    }

    fn parse_seq(&mut self, stop: Stop) -> String {
        let mut out = String::new();
        while !self.rest().is_empty() && !self.at_stop(stop) {
            out.push_str(&self.atom_with_scripts());
        }
        out
    }

    fn at_stop(&self, stop: Stop) -> bool {
        let rest = self.rest();
        match stop {
            Stop::End => false,
            Stop::Brace => rest.starts_with('}'),
            Stop::Right => rest.starts_with("\\right"),
            Stop::Cell => {
                rest.starts_with('&') || rest.starts_with("\\\\") || rest.starts_with("\\end")
            }
        }
    }

    fn atom_with_scripts(&mut self) -> String {
        let base = self.atom();
        let mut sub = None;
        let mut sup = None;
        if self.eat("_") {
            sub = Some(self.script_body());
        }
        if self.eat("^") {
            sup = Some(self.script_body());
        }
        if sub.is_none() && self.eat("_") {
            sub = Some(self.script_body());
        }
        match (sub, sup) {
            (None, None) => base,
            (Some(sub), None) => {
                format!("<m:sSub><m:e>{base}</m:e><m:sub>{sub}</m:sub></m:sSub>")
            }
            (None, Some(sup)) => {
                format!("<m:sSup><m:e>{base}</m:e><m:sup>{sup}</m:sup></m:sSup>")
            }
            (Some(sub), Some(sup)) => format!(
                "<m:sSubSup><m:e>{base}</m:e><m:sub>{sub}</m:sub><m:sup>{sup}</m:sup></m:sSubSup>"
            ),
        }
    }

    fn script_body(&mut self) -> String {
        if self.rest().starts_with('{') {
            self.group()
        } else {
            self.atom()
        }
    }

    fn atom(&mut self) -> String {
        if self.eat("{") {
            let inner = self.parse_seq(Stop::Brace);
            let _ = self.eat("}");
            return inner;
        }
        if self.eat("&") {
            return "<m:aln/>".to_string();
        }
        if self.rest().starts_with('\\') {
            return self.command();
        }
        self.text_run()
    }

    fn group(&mut self) -> String {
        if self.eat("{") {
            let inner = self.parse_seq(Stop::Brace);
            let _ = self.eat("}");
            inner
        } else {
            self.atom()
        }
    }

    fn command(&mut self) -> String {
        let name = self.command_name();
        match name.as_str() {
            "frac" => {
                let num = self.group();
                let den = self.group();
                format!("<m:f><m:num>{num}</m:num><m:den>{den}</m:den></m:f>")
            }
            "sqrt" => self.radical(),
            "text" => run_xml("", "", true, &unescape_text(&self.group_text())),
            "mathrm" => run_xml("p", "", false, &self.group_text()),
            "mathbf" => run_xml("b", "", false, &self.group_text()),
            "boldsymbol" => {
                if self.rest().starts_with("\\mathcal")
                    || self.rest().starts_with("\\mathfrak")
                    || self.rest().starts_with("\\mathbb")
                    || self.rest().starts_with("\\mathsf")
                    || self.rest().starts_with("\\mathtt")
                {
                    let inner = self.command();
                    inner.replacen("<m:rPr>", "<m:rPr><m:sty m:val=\"b\"/>", 1)
                } else {
                    run_xml("bi", "", false, &self.group_text())
                }
            }
            "mathbb" => run_xml("", "double-struck", false, &self.group_text()),
            "mathcal" => run_xml("", "script", false, &self.group_text()),
            "mathfrak" => run_xml("", "fraktur", false, &self.group_text()),
            "mathsf" => run_xml("", "sans-serif", false, &self.group_text()),
            "mathtt" => run_xml("", "monospace", false, &self.group_text()),
            "left" => self.delimiter(),
            "right" => String::new(),
            "begin" => self.environment(),
            "end" => {
                let _ = self.group_text();
                String::new()
            }
            "overline" => bar(false, &self.group()),
            "underline" => bar(true, &self.group()),
            "boxed" => format!("<m:borderBox><m:e>{}</m:e></m:borderBox>", self.group()),
            "overbrace" => group_chr("⏞", false, &self.group()),
            "underbrace" => group_chr("⏟", true, &self.group()),
            "overset" => {
                let top = self.group();
                let body = self.group();
                format!("<m:limUpp><m:e>{body}</m:e><m:lim>{top}</m:lim></m:limUpp>")
            }
            "underset" => {
                let bot = self.group();
                let body = self.group();
                format!("<m:limLow><m:e>{body}</m:e><m:lim>{bot}</m:lim></m:limLow>")
            }
            "phantom" => phantom("", &self.group()),
            "vphantom" => phantom("<m:zeroWid m:val=\"1\"/>", &self.group()),
            "hphantom" => phantom(
                "<m:zeroAsc m:val=\"1\"/><m:zeroDesc m:val=\"1\"/>",
                &self.group(),
            ),
            "smash" => self.smash(),
            "cancel" => strike("<m:strikeTLBR m:val=\"1\"/>", &self.group()),
            "bcancel" => strike("<m:strikeBLTR m:val=\"1\"/>", &self.group()),
            "xcancel" => strike(
                "<m:strikeTLBR m:val=\"1\"/><m:strikeBLTR m:val=\"1\"/>",
                &self.group(),
            ),
            "genfrac" => self.genfrac(),
            "textstyle" | "displaystyle" | "scriptstyle" | "scriptscriptstyle" => self.atom(),
            "\\" => "<m:brk/>".to_string(),
            other if nary_char(other).is_some() => self.nary(other),
            other if function_name(other) => run_xml("p", "", false, other),
            other => {
                if let Some(ch) = symbol_char(other) {
                    run_xml("", "", false, &ch.to_string())
                } else {
                    run_xml("", "", false, other)
                }
            }
        }
    }

    fn radical(&mut self) -> String {
        if self.eat("[") {
            let deg = self.parse_until_bracket();
            return format!(
                "<m:rad><m:deg>{deg}</m:deg><m:e>{}</m:e></m:rad>",
                self.group()
            );
        }
        format!("<m:rad><m:deg></m:deg><m:e>{}</m:e></m:rad>", self.group())
    }

    fn parse_until_bracket(&mut self) -> String {
        let start = self.i;
        let mut depth = 0i32;
        while self.i < self.src.len() {
            let ch = self.src[self.i..].chars().next().unwrap_or(']');
            if ch == '{' {
                depth += 1;
            } else if ch == '}' && depth > 0 {
                depth -= 1;
            } else if ch == ']' && depth == 0 {
                let inner = &self.src[start..self.i];
                self.i += 1;
                return Parser { src: inner, i: 0 }.parse_seq(Stop::End);
            }
            self.i += ch.len_utf8();
        }
        String::new()
    }

    fn delimiter(&mut self) -> String {
        let begin = self.fence();
        let inner = self.parse_seq(Stop::Right);
        let _ = self.eat("\\right");
        let end = self.fence();
        format!(
            "<m:d><m:dPr><m:begChr m:val=\"{begin}\"/><m:endChr m:val=\"{end}\"/></m:dPr><m:e>{inner}</m:e></m:d>"
        )
    }

    fn fence(&mut self) -> String {
        if self.eat("\\") {
            let name = self.command_name();
            return match name.as_str() {
                "{" => "{".to_string(),
                "}" => "}".to_string(),
                "|" => "‖".to_string(),
                "langle" => "⟨".to_string(),
                "rangle" => "⟩".to_string(),
                "lceil" => "⌈".to_string(),
                "rceil" => "⌉".to_string(),
                "lfloor" => "⌊".to_string(),
                "rfloor" => "⌋".to_string(),
                "." => String::new(),
                other => other.to_string(),
            };
        }
        let ch = self.rest().chars().next().unwrap_or('.');
        if ch == '.' {
            self.i += 1;
            return String::new();
        }
        self.i += ch.len_utf8();
        if ch == '.' {
            String::new()
        } else {
            xml_attr(ch)
        }
    }

    fn environment(&mut self) -> String {
        let name = self.group_text();
        match name.as_str() {
            "matrix" => self.matrix(""),
            "array" => {
                let spec = self.group_text();
                self.matrix(&spec)
            }
            "aligned" => self.aligned(),
            _ => self.parse_seq(Stop::End),
        }
    }

    fn matrix(&mut self, spec: &str) -> String {
        let rows = self.grid_until_end();
        let mut out = String::from("<m:m>");
        if spec.chars().any(|ch| ch == 'l' || ch == 'r') {
            out.push_str("<m:mPr><m:mcs>");
            for ch in spec.chars() {
                let align = match ch {
                    'l' => "left",
                    'r' => "right",
                    _ => "center",
                };
                out.push_str(&format!(
                    "<m:mc><m:mcPr><m:mcJc m:val=\"{align}\"/></m:mcPr></m:mc>"
                ));
            }
            out.push_str("</m:mcs></m:mPr>");
        }
        for row in rows {
            out.push_str("<m:mr>");
            for cell in row {
                out.push_str(&format!("<m:e>{cell}</m:e>"));
            }
            out.push_str("</m:mr>");
        }
        out.push_str("</m:m>");
        out
    }

    fn aligned(&mut self) -> String {
        let rows = self.grid_until_end();
        let mut out = String::from("<m:eqArr>");
        for row in rows {
            out.push_str("<m:e>");
            out.push_str(&row.join("<m:aln/>"));
            out.push_str("</m:e>");
        }
        out.push_str("</m:eqArr>");
        out
    }

    fn grid_until_end(&mut self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        let mut row = Vec::new();
        while !self.rest().is_empty() && !self.rest().starts_with("\\end") {
            let cell = self.parse_seq(Stop::Cell);
            if self.eat("&") {
                row.push(cell);
                continue;
            }
            row.push(cell);
            if self.eat("\\\\") {
                rows.push(std::mem::take(&mut row));
                continue;
            }
            break;
        }
        if self.rest().starts_with("\\end") {
            let _ = self.eat("\\end");
            let _ = self.group_text();
        }
        if !row.is_empty() {
            rows.push(row);
        }
        rows
    }

    fn nary(&mut self, name: &str) -> String {
        let chr = nary_char(name).unwrap_or('∫');
        let mut limits = false;
        if self.rest().starts_with("\\limits") {
            let _ = self.eat("\\limits");
            limits = true;
        }
        let mut sub = String::new();
        let mut sup = String::new();
        if self.eat("_") {
            sub = self.script_body();
        }
        if self.eat("^") {
            sup = self.script_body();
        }
        let body = if self.rest().starts_with('{') {
            self.group()
        } else if self.rest().is_empty() || self.at_stop(Stop::Cell) || self.at_stop(Stop::Right) {
            String::new()
        } else {
            self.atom()
        };
        let lim = if limits {
            "<m:limLoc m:val=\"undOvr\"/>"
        } else {
            ""
        };
        let sub_xml = if sub.is_empty() {
            String::new()
        } else {
            format!("<m:sub>{sub}</m:sub>")
        };
        let sup_xml = if sup.is_empty() {
            String::new()
        } else {
            format!("<m:sup>{sup}</m:sup>")
        };
        format!(
            "<m:nary><m:naryPr><m:chr m:val=\"{chr}\"/>{lim}</m:naryPr>{sub_xml}{sup_xml}<m:e>{body}</m:e></m:nary>"
        )
    }

    fn smash(&mut self) -> String {
        let mut kind = "t";
        if self.eat("[") {
            kind = if self.eat("b") { "b" } else { "t" };
            let _ = self.eat("]");
        }
        let body = self.group();
        if kind == "b" {
            phantom("<m:zeroDesc m:val=\"1\"/>", &body)
        } else {
            phantom("<m:zeroAsc m:val=\"1\"/>", &body)
        }
    }

    fn genfrac(&mut self) -> String {
        let _left = self.group();
        let _right = self.group();
        let thickness = self.group_text();
        let _style = self.group();
        let num = self.group();
        let den = self.group();
        if thickness.contains("0pt") {
            format!("<m:f><m:fPr><m:type m:val=\"noBar\"/></m:fPr><m:num>{num}</m:num><m:den>{den}</m:den></m:f>")
        } else {
            format!("<m:f><m:num>{num}</m:num><m:den>{den}</m:den></m:f>")
        }
    }

    fn group_text(&mut self) -> String {
        if !self.eat("{") {
            return String::new();
        }
        let start = self.i;
        let mut depth = 1i32;
        while self.i < self.src.len() && depth > 0 {
            let ch = self.src[self.i..].chars().next().unwrap_or('}');
            self.i += ch.len_utf8();
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        let end = if depth == 0 { self.i - 1 } else { self.i };
        self.src[start..end].to_string()
    }

    fn command_name(&mut self) -> String {
        let _ = self.eat("\\");
        let rest = self.rest();
        let mut len = 0;
        for ch in rest.chars() {
            if ch.is_ascii_alphabetic() {
                len += 1;
            } else {
                break;
            }
        }
        if len == 0 {
            let ch = rest.chars().next().unwrap_or(' ');
            self.i += ch.len_utf8();
            return ch.to_string();
        }
        let name = rest[..len].to_string();
        self.i += len;
        name
    }

    fn text_run(&mut self) -> String {
        let start = self.i;
        while self.i < self.src.len() {
            let ch = self.src[self.i..].chars().next().unwrap_or(' ');
            if matches!(ch, '\\' | '{' | '}' | '_' | '^' | '&') {
                break;
            }
            self.i += ch.len_utf8();
        }
        run_xml("", "", false, &self.src[start..self.i])
    }
}

fn run_xml(style: &str, script: &str, literal: bool, text: &str) -> String {
    if text.is_empty() && style.is_empty() && script.is_empty() && !literal {
        return String::new();
    }
    let mut pr = String::new();
    if !style.is_empty() {
        pr.push_str(&format!("<m:sty m:val=\"{style}\"/>"));
    }
    if !script.is_empty() {
        pr.push_str(&format!("<m:scr m:val=\"{script}\"/>"));
    }
    if literal {
        pr.push_str("<m:nor/>");
    }
    let pr = if pr.is_empty() {
        String::new()
    } else {
        format!("<m:rPr>{pr}</m:rPr>")
    };
    format!(
        "<m:r>{pr}<m:t xml:space=\"preserve\">{}</m:t></m:r>",
        xml_text(text)
    )
}

fn bar(under: bool, body: &str) -> String {
    let pos = if under {
        "<m:barPr><m:pos m:val=\"bot\"/></m:barPr>"
    } else {
        ""
    };
    format!("<m:bar>{pos}<m:e>{body}</m:e></m:bar>")
}

fn group_chr(chr: &str, under: bool, body: &str) -> String {
    let chr = xml_attr_str(chr);
    let pos = if under {
        format!("<m:groupChrPr><m:chr m:val=\"{chr}\"/><m:pos m:val=\"bot\"/></m:groupChrPr>")
    } else {
        format!("<m:groupChrPr><m:chr m:val=\"{chr}\"/></m:groupChrPr>")
    };
    format!("<m:groupChr>{pos}<m:e>{body}</m:e></m:groupChr>")
}

fn phantom(flags: &str, body: &str) -> String {
    format!(
        "<m:phant><m:phantPr><m:show m:val=\"0\"/>{flags}</m:phantPr><m:e>{body}</m:e></m:phant>"
    )
}

fn strike(flags: &str, body: &str) -> String {
    format!(
        "<m:borderBox><m:borderBoxPr><m:hideTop m:val=\"1\"/><m:hideBot m:val=\"1\"/><m:hideLeft m:val=\"1\"/><m:hideRight m:val=\"1\"/>{flags}</m:borderBoxPr><m:e>{body}</m:e></m:borderBox>"
    )
}

fn nary_char(name: &str) -> Option<char> {
    Some(match name {
        "int" => '∫',
        "iint" => '∬',
        "iiint" => '∭',
        "oint" => '∮',
        "oiint" => '∯',
        "sum" => '∑',
        "prod" => '∏',
        "coprod" => '∐',
        "bigwedge" => '⋀',
        "bigvee" => '⋁',
        "bigcap" => '⋂',
        "bigcup" => '⋃',
        "bigoplus" => '⨁',
        "bigotimes" => '⨂',
        "bigodot" => '⨀',
        "biguplus" => '⨄',
        _ => return None,
    })
}

fn function_name(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "ln"
            | "lg"
            | "exp"
            | "lim"
            | "liminf"
            | "limsup"
            | "max"
            | "min"
            | "sup"
            | "inf"
            | "det"
            | "dim"
            | "gcd"
            | "arg"
            | "ker"
            | "hom"
            | "deg"
            | "Pr"
    )
}

fn symbol_char(name: &str) -> Option<char> {
    Some(match name {
        "pm" => '±',
        "mp" => '∓',
        "times" => '×',
        "div" => '÷',
        "cdot" => '·',
        "circ" => '∘',
        "ast" => '∗',
        "bullet" => '∙',
        "leq" => '≤',
        "geq" => '≥',
        "neq" => '≠',
        "approx" => '≈',
        "equiv" => '≡',
        "sim" => '∼',
        "cong" => '≅',
        "propto" => '∝',
        "infty" => '∞',
        "partial" => '∂',
        "nabla" => '∇',
        "in" => '∈',
        "notin" => '∉',
        "ni" => '∋',
        "subset" => '⊂',
        "supset" => '⊃',
        "subseteq" => '⊆',
        "supseteq" => '⊇',
        "cup" => '∪',
        "cap" => '∩',
        "wedge" => '∧',
        "vee" => '∨',
        "neg" => '¬',
        "forall" => '∀',
        "exists" => '∃',
        "emptyset" => '∅',
        "oplus" => '⊕',
        "otimes" => '⊗',
        "perp" => '⊥',
        "parallel" => '∥',
        "angle" => '∠',
        "ldots" => '…',
        "rightarrow" => '→',
        "leftarrow" => '←',
        "leftrightarrow" => '↔',
        "Rightarrow" => '⇒',
        "Leftarrow" => '⇐',
        "Leftrightarrow" => '⇔',
        "alpha" => 'α',
        "beta" => 'β',
        "gamma" => 'γ',
        "delta" => 'δ',
        "varepsilon" => 'ε',
        "epsilon" => 'ϵ',
        "zeta" => 'ζ',
        "eta" => 'η',
        "theta" => 'θ',
        "iota" => 'ι',
        "kappa" => 'κ',
        "lambda" => 'λ',
        "mu" => 'μ',
        "nu" => 'ν',
        "xi" => 'ξ',
        "pi" => 'π',
        "rho" => 'ρ',
        "sigma" => 'σ',
        "tau" => 'τ',
        "upsilon" => 'υ',
        "phi" => 'φ',
        "chi" => 'χ',
        "psi" => 'ψ',
        "omega" => 'ω',
        "Gamma" => 'Γ',
        "Delta" => 'Δ',
        "Theta" => 'Θ',
        "Lambda" => 'Λ',
        "Xi" => 'Ξ',
        "Pi" => 'Π',
        "Sigma" => 'Σ',
        "Upsilon" => 'Υ',
        "Phi" => 'Φ',
        "Psi" => 'Ψ',
        "Omega" => 'Ω',
        "prime" => '′',
        "therefore" => '∴',
        "because" => '∵',
        "ll" => '≪',
        "gg" => '≫',
        "ell" => 'ℓ',
        "hbar" => 'ℏ',
        "wp" => '℘',
        "backslash" => '\\',
        "{" => '{',
        "}" => '}',
        "%" => '%',
        "$" => '$',
        "&" => '&',
        "#" => '#',
        "_" => '_',
        _ => return None,
    })
}

fn unescape_text(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn xml_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn xml_attr(ch: char) -> String {
    xml_attr_str(&ch.to_string())
}

fn xml_attr_str(text: &str) -> String {
    xml_text(text).replace('"', "&quot;")
}
