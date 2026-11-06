//! Office Math (`m:oMath`) to LaTeX, without the surrounding `$` delimiters.

use edgedoc_opc::xml::El;

pub(crate) fn to_latex(el: &El) -> String {
    convert(el).trim().to_string()
}

fn convert(el: &El) -> String {
    match el.name.as_str() {
        "t" => math_text(&el.text()),
        "r" => convert_run(el),
        "f" => convert_fraction(el),
        "sSup" => scripts(&part(el, "e"), "", &part(el, "sup")),
        "sSub" => scripts(&part(el, "e"), &part(el, "sub"), ""),
        "sSubSup" => scripts(&part(el, "e"), &part(el, "sub"), &part(el, "sup")),
        "sPre" => convert_pre(el),
        "oMathPara" => convert_display(el),
        "e" | "sup" | "sub" | "num" | "den" | "deg" | "lim" | "fName" => {
            sized(el, convert_children(el))
        }
        "rad" => convert_radical(el),
        "nary" => convert_nary(el),
        "d" => convert_delimiter(el),
        "m" => convert_matrix(el),
        "func" => format!("{}{}", part(el, "fName"), part(el, "e")),
        "limLow" => format!("\\underset{{{}}}{{{}}}", part(el, "lim"), part(el, "e")),
        "limUpp" => format!("\\overset{{{}}}{{{}}}", part(el, "lim"), part(el, "e")),
        "acc" => convert_accent(el),
        "bar" => {
            let under = prop_val(el, "pos").as_deref() == Some("bot");
            let cmd = if under { "\\underline" } else { "\\overline" };
            format!("{cmd}{{{}}}", part(el, "e"))
        }
        "groupChr" => convert_group(el),
        "borderBox" => convert_border(el),
        "phant" => convert_phantom(el),
        "eqArr" => {
            let rows: Vec<String> = el
                .child_els()
                .filter(|child| child.name == "e")
                .map(convert)
                .collect();
            format!("\\begin{{aligned}}{}\\end{{aligned}}", rows.join("\\\\"))
        }
        "mr" => el
            .child_els()
            .filter(|child| child.name == "e")
            .map(convert)
            .collect::<Vec<_>>()
            .join("&"),
        "aln" => "&".to_string(),
        "brk" => "\\\\".to_string(),
        _ => convert_children(el),
    }
}

fn convert_children(el: &El) -> String {
    let mut out = String::new();
    for child in el.child_els() {
        if child.name.ends_with("Pr") {
            continue;
        }
        out.push_str(&convert(child));
    }
    out
}

fn part(el: &El, name: &str) -> String {
    el.child_els()
        .find(|child| child.name == name)
        .map(convert)
        .unwrap_or_default()
}

fn props(el: &El) -> Option<&El> {
    el.child_els().find(|child| child.name.ends_with("Pr"))
}

fn prop<'a>(el: &'a El, name: &str) -> Option<&'a El> {
    props(el)?.child_els().find(|child| child.name == name)
}

fn prop_val(el: &El, name: &str) -> Option<String> {
    prop(el, name)
        .and_then(|child| child.attr("val"))
        .map(str::to_string)
}

/// A present flag is on unless its value says otherwise. A missing flag is off.
fn flag(el: &El, name: &str) -> bool {
    flag_on(el, name, false)
}

fn flag_on(el: &El, name: &str, default: bool) -> bool {
    match prop(el, name) {
        None => default,
        Some(child) => !matches!(child.attr("val"), Some("0" | "false" | "off")),
    }
}

fn convert_run(el: &El) -> String {
    let pr = el.child_els().find(|child| child.name == "rPr");
    let normal = pr.is_some_and(|pr| {
        pr.child_els().any(|child| {
            child.name == "nor" && !matches!(child.attr("val"), Some("0" | "false" | "off"))
        })
    });
    let literal = pr.is_some_and(|pr| {
        pr.child_els().any(|child| {
            child.name == "lit" && !matches!(child.attr("val"), Some("0" | "false" | "off"))
        })
    });
    let style = pr
        .and_then(|pr| {
            pr.child_els()
                .find(|child| child.name == "sty")
                .and_then(|child| child.attr("val"))
        })
        .unwrap_or("i");
    let script = pr.and_then(|pr| {
        pr.child_els()
            .find(|child| child.name == "scr")
            .and_then(|child| child.attr("val"))
            .map(str::to_string)
    });
    let mut out = String::new();
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut String| {
        if text.is_empty() {
            return;
        }
        out.push_str(&style_text(
            text,
            normal || literal,
            style,
            script.as_deref(),
        ));
        text.clear();
    };
    for child in el.child_els() {
        match child.name.as_str() {
            "rPr" => {}
            "t" => text.push_str(&child.text()),
            "aln" => {
                flush(&mut text, &mut out);
                out.push('&');
            }
            "brk" => {
                flush(&mut text, &mut out);
                out.push_str("\\\\");
            }
            _ => {
                flush(&mut text, &mut out);
                out.push_str(&convert(child));
            }
        }
    }
    flush(&mut text, &mut out);
    out
}

fn style_text(text: &str, literal: bool, style: &str, script: Option<&str>) -> String {
    if literal {
        return format!("\\text{{{}}}", text_mode(text));
    }
    let roman = matches!(script, None | Some("roman"));
    if style == "p" && roman {
        if let Some(cmd) = function_command(text.trim()) {
            return cmd.to_string();
        }
    }
    let mapped = math_text(text);
    if mapped.contains('\\') {
        return mapped;
    }
    wrap_alphabet(style, script, &mapped)
}

fn wrap_alphabet(style: &str, script: Option<&str>, mapped: &str) -> String {
    let script = script.unwrap_or("");
    let cmd = match script {
        "double-struck" => "\\mathbb",
        "script" => "\\mathcal",
        "fraktur" => "\\mathfrak",
        "sans-serif" => "\\mathsf",
        "monospace" => "\\mathtt",
        _ => match style {
            "p" => "\\mathrm",
            "b" => "\\mathbf",
            "bi" => "\\boldsymbol",
            _ => return mapped.to_string(),
        },
    };
    if matches!(style, "b" | "bi") && !matches!(script, "" | "roman") {
        format!("\\boldsymbol{{{cmd}{{{mapped}}}}}")
    } else {
        format!("{cmd}{{{mapped}}}")
    }
}

fn scripts(base: &str, sub: &str, sup: &str) -> String {
    let mut out = format!("{{{base}}}");
    if !sub.is_empty() {
        out.push_str(&format!("_{{{sub}}}"));
    }
    if !sup.is_empty() {
        out.push_str(&format!("^{{{sup}}}"));
    }
    out
}

fn convert_fraction(el: &El) -> String {
    let kind = prop_val(el, "type").unwrap_or_else(|| "bar".to_string());
    let num = part(el, "num");
    let den = part(el, "den");
    match kind.as_str() {
        "noBar" => format!("\\genfrac{{}}{{}}{{0pt}}{{}}{{{num}}}{{{den}}}"),
        "skw" => format!("{{}}^{{{num}}}/_{{{den}}}"),
        "lin" => format!("{{{num}}}/{{{den}}}"),
        _ => format!("\\frac{{{num}}}{{{den}}}"),
    }
}

fn convert_radical(el: &El) -> String {
    let body = part(el, "e");
    let degree = part(el, "deg");
    if flag(el, "degHide") || degree.is_empty() {
        format!("\\sqrt{{{body}}}")
    } else {
        format!("\\sqrt[{degree}]{{{body}}}")
    }
}

fn convert_nary(el: &El) -> String {
    let chr = prop(el, "chr").and_then(|child| child.attr("val"));
    let mut out = nary_command(chr);
    if prop_val(el, "limLoc").as_deref() == Some("undOvr") {
        out.push_str("\\limits");
    }
    if !flag(el, "subHide") {
        let sub = part(el, "sub");
        if !sub.is_empty() {
            out.push_str(&format!("_{{{sub}}}"));
        }
    }
    if !flag(el, "supHide") {
        let sup = part(el, "sup");
        if !sup.is_empty() {
            out.push_str(&format!("^{{{sup}}}"));
        }
    }
    out.push_str(&format!("{{{}}}", part(el, "e")));
    if flag_on(el, "grow", true) {
        out
    } else {
        format!("{{\\textstyle {out}}}")
    }
}

fn nary_command(chr: Option<&str>) -> String {
    match chr {
        None => "\\int".to_string(),
        Some("") => String::new(),
        Some("∑") => "\\sum".to_string(),
        Some("∏") => "\\prod".to_string(),
        Some("∐") => "\\coprod".to_string(),
        Some("∫") => "\\int".to_string(),
        Some("∬") => "\\iint".to_string(),
        Some("∭") => "\\iiint".to_string(),
        Some("∮") => "\\oint".to_string(),
        Some("∯") => "\\oiint".to_string(),
        Some("⋀") => "\\bigwedge".to_string(),
        Some("⋁") => "\\bigvee".to_string(),
        Some("⋂") => "\\bigcap".to_string(),
        Some("⋃") => "\\bigcup".to_string(),
        Some("⨁") => "\\bigoplus".to_string(),
        Some("⨂") => "\\bigotimes".to_string(),
        Some("⨀") => "\\bigodot".to_string(),
        Some("⨄") => "\\biguplus".to_string(),
        Some(other) => math_text(other),
    }
}

fn convert_delimiter(el: &El) -> String {
    let grow = match prop(el, "grow") {
        None => true,
        Some(node) => !matches!(node.attr("val"), Some("0" | "false" | "off")),
    };
    let begin = fence(&delimiter_char(el, "begChr", "("));
    let end = fence(&delimiter_char(el, "endChr", ")"));
    let sep = math_text(&delimiter_char(el, "sepChr", ","));
    let parts: Vec<String> = el
        .child_els()
        .filter(|child| child.name == "e")
        .map(convert)
        .collect();
    let inner = parts.join(&sep);
    if grow {
        format!("\\left{begin}{inner}\\right{end}")
    } else {
        format!("{begin}{inner}{end}")
    }
}

fn delimiter_char(el: &El, name: &str, default: &str) -> String {
    match prop(el, name) {
        None => default.to_string(),
        Some(child) => child.attr("val").unwrap_or("").to_string(),
    }
}

fn fence(ch: &str) -> String {
    match ch {
        "" => ".".to_string(),
        "(" | ")" | "[" | "]" | "|" | "/" | "." => ch.to_string(),
        "{" => "\\{".to_string(),
        "}" => "\\}".to_string(),
        "‖" | "||" => "\\|".to_string(),
        "⟨" => "\\langle ".to_string(),
        "⟩" => "\\rangle ".to_string(),
        "⌈" => "\\lceil ".to_string(),
        "⌉" => "\\rceil ".to_string(),
        "⌊" => "\\lfloor ".to_string(),
        "⌋" => "\\rfloor ".to_string(),
        other => math_text(other),
    }
}

fn convert_matrix(el: &El) -> String {
    let spec = matrix_spec(el);
    let rows: Vec<String> = el
        .child_els()
        .filter(|child| child.name == "mr")
        .map(|row| {
            let mut cells: Vec<String> = row
                .child_els()
                .filter(|child| child.name == "e")
                .map(convert)
                .collect();
            if cells.len() < spec.len() {
                cells.resize(spec.len(), String::new());
            }
            cells.join("&")
        })
        .collect();
    let body = rows.join("\\\\");
    if spec.is_empty() || spec.iter().all(|align| *align == 'c') {
        format!("\\begin{{matrix}}{body}\\end{{matrix}}")
    } else {
        let cols: String = spec.into_iter().collect();
        format!("\\begin{{array}}{{{cols}}}{body}\\end{{array}}")
    }
}

fn matrix_spec(el: &El) -> Vec<char> {
    let mut spec = Vec::new();
    let Some(pr) = props(el) else {
        return spec;
    };
    let Some(mcs) = pr.child_els().find(|child| child.name == "mcs") else {
        return spec;
    };
    for mc in mcs.child_els().filter(|child| child.name == "mc") {
        let pr = mc.child_els().find(|child| child.name == "mcPr");
        let count = pr
            .and_then(|pr| pr.child_els().find(|child| child.name == "count"))
            .and_then(|child| child.attr("val"))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 64);
        let align = pr
            .and_then(|pr| pr.child_els().find(|child| child.name == "mcJc"))
            .and_then(|child| child.attr("val"));
        let column = match align {
            Some("left") => 'l',
            Some("right") => 'r',
            _ => 'c',
        };
        for _ in 0..count {
            spec.push(column);
        }
    }
    spec
}

fn convert_accent(el: &El) -> String {
    let body = part(el, "e");
    let chr = prop_val(el, "chr").unwrap_or_else(|| "\u{0302}".to_string());
    if let Some(cmd) = accent_command(&chr) {
        format!("{cmd}{{{body}}}")
    } else {
        format!("\\overset{{{}}}{{{body}}}", math_text(&chr))
    }
}

fn accent_command(chr: &str) -> Option<&'static str> {
    Some(match chr {
        "\u{0302}" | "^" | "ˆ" => "\\hat",
        "\u{0303}" | "~" => "\\tilde",
        "\u{0304}" | "\u{0305}" | "¯" => "\\bar",
        "\u{20D7}" => "\\vec",
        "\u{0307}" => "\\dot",
        "\u{0308}" => "\\ddot",
        "\u{20DB}" => "\\dddot",
        "\u{030C}" => "\\check",
        "\u{0301}" => "\\acute",
        "\u{0300}" => "\\grave",
        "\u{0306}" => "\\breve",
        "\u{030A}" => "\\mathring",
        _ => return None,
    })
}

fn convert_group(el: &El) -> String {
    let chr = prop_val(el, "chr").unwrap_or_default();
    let body = part(el, "e");
    match chr.as_str() {
        "⏞" => format!("\\overbrace{{{body}}}"),
        "⏟" => format!("\\underbrace{{{body}}}"),
        _ if prop_val(el, "pos").as_deref() == Some("bot") => {
            format!("\\underset{{{}}}{{{body}}}", math_text(&chr))
        }
        _ => format!("\\overset{{{}}}{{{body}}}", math_text(&chr)),
    }
}

fn convert_phantom(el: &El) -> String {
    let body = part(el, "e");
    let show = match prop(el, "show") {
        None => true,
        Some(node) => !matches!(node.attr("val"), Some("0" | "false" | "off")),
    };
    if show {
        return body;
    }
    let width = flag(el, "zeroWid");
    let ascent = flag(el, "zeroAsc");
    let descent = flag(el, "zeroDesc");
    if width && ascent && descent {
        return String::new();
    }
    if width && ascent && !descent {
        return format!("\\smash[t]{{\\vphantom{{{body}}}}}");
    }
    if width && !ascent && descent {
        return format!("\\smash[b]{{\\vphantom{{{body}}}}}");
    }
    if width {
        return format!("\\vphantom{{{body}}}");
    }
    if ascent && descent {
        return format!("\\hphantom{{{body}}}");
    }
    if ascent {
        return format!("\\smash[t]{{\\phantom{{{body}}}}}");
    }
    if descent {
        return format!("\\smash[b]{{\\phantom{{{body}}}}}");
    }
    format!("\\phantom{{{body}}}")
}

fn convert_pre(el: &El) -> String {
    let base = part(el, "e");
    let sub = part(el, "sub");
    let sup = part(el, "sup");
    if sub.is_empty() && sup.is_empty() {
        return base;
    }
    let mut out = String::from("{}");
    if !sub.is_empty() {
        out.push_str(&format!("_{{{sub}}}"));
    }
    if !sup.is_empty() {
        out.push_str(&format!("^{{{sup}}}"));
    }
    out.push_str(&format!("{{{base}}}"));
    out
}

fn convert_display(el: &El) -> String {
    let body = convert_children(el);
    match prop_val(el, "jc").as_deref() {
        Some("left") => format!("\\begin{{aligned}}&{body}\\end{{aligned}}"),
        Some("right") => format!("\\begin{{aligned}}{body}&\\end{{aligned}}"),
        _ => body,
    }
}

fn convert_border(el: &El) -> String {
    let body = part(el, "e");
    let top = !flag(el, "hideTop");
    let bottom = !flag(el, "hideBot");
    let left = !flag(el, "hideLeft");
    let right = !flag(el, "hideRight");
    let mut out = if top && bottom && left && right {
        format!("\\boxed{{{body}}}")
    } else if top && !bottom && !left && !right {
        format!("\\overline{{{body}}}")
    } else if bottom && !top && !left && !right {
        format!("\\underline{{{body}}}")
    } else if top && bottom && !left && !right {
        format!("\\overline{{\\underline{{{body}}}}}")
    } else if !top && !bottom && !left && !right {
        body
    } else {
        format!("\\boxed{{{body}}}")
    };
    if flag(el, "strikeTLBR") && flag(el, "strikeBLTR") {
        out = format!("\\xcancel{{{out}}}");
    } else if flag(el, "strikeTLBR") {
        out = format!("\\cancel{{{out}}}");
    } else if flag(el, "strikeBLTR") {
        out = format!("\\bcancel{{{out}}}");
    }
    if flag(el, "strikeH") {
        out = format!("\\overline{{\\underline{{{out}}}}}");
    }
    if flag(el, "strikeV") {
        out = format!("\\left.{out}\\middle|\\right.");
    }
    out
}

fn sized(el: &El, body: String) -> String {
    let Some(raw) = prop_val(el, "argSz") else {
        return body;
    };
    let Ok(size) = raw.parse::<i32>() else {
        return body;
    };
    let style = if size <= -2 {
        "\\scriptscriptstyle"
    } else if size == -1 {
        "\\scriptstyle"
    } else if size == 1 {
        "\\textstyle"
    } else if size >= 2 {
        "\\displaystyle"
    } else {
        return body;
    };
    format!("{{{style} {body}}}")
}

fn function_command(text: &str) -> Option<&'static str> {
    Some(match text {
        "sin" => "\\sin ",
        "cos" => "\\cos ",
        "tan" => "\\tan ",
        "cot" => "\\cot ",
        "sec" => "\\sec ",
        "csc" => "\\csc ",
        "arcsin" => "\\arcsin ",
        "arccos" => "\\arccos ",
        "arctan" => "\\arctan ",
        "sinh" => "\\sinh ",
        "cosh" => "\\cosh ",
        "tanh" => "\\tanh ",
        "log" => "\\log ",
        "ln" => "\\ln ",
        "lg" => "\\lg ",
        "exp" => "\\exp ",
        "lim" => "\\lim ",
        "liminf" => "\\liminf ",
        "limsup" => "\\limsup ",
        "max" => "\\max ",
        "min" => "\\min ",
        "sup" => "\\sup ",
        "inf" => "\\inf ",
        "det" => "\\det ",
        "dim" => "\\dim ",
        "gcd" => "\\gcd ",
        "arg" => "\\arg ",
        "ker" => "\\ker ",
        "hom" => "\\hom ",
        "deg" => "\\deg ",
        "Pr" => "\\Pr ",
        _ => return None,
    })
}

fn math_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        push_math_char(&mut out, ch);
    }
    out
}

fn push_math_char(out: &mut String, ch: char) {
    let command = match ch {
        '\\' => "\\backslash ",
        '{' => "\\{",
        '}' => "\\}",
        '$' => "\\$",
        '%' => "\\%",
        '&' => "\\&",
        '#' => "\\#",
        '_' => "\\_",
        '^' => "\\^{}",
        '~' => "\\~{}",
        '−' => "-",
        '±' => "\\pm ",
        '∓' => "\\mp ",
        '×' => "\\times ",
        '÷' => "\\div ",
        '·' => "\\cdot ",
        '∘' => "\\circ ",
        '∗' => "\\ast ",
        '∙' => "\\bullet ",
        '≤' => "\\leq ",
        '≥' => "\\geq ",
        '≠' => "\\neq ",
        '≈' => "\\approx ",
        '≡' => "\\equiv ",
        '∼' => "\\sim ",
        '≅' => "\\cong ",
        '∝' => "\\propto ",
        '∞' => "\\infty ",
        '∂' => "\\partial ",
        '∇' => "\\nabla ",
        '∈' => "\\in ",
        '∉' => "\\notin ",
        '∋' => "\\ni ",
        '⊂' => "\\subset ",
        '⊃' => "\\supset ",
        '⊆' => "\\subseteq ",
        '⊇' => "\\supseteq ",
        '∪' => "\\cup ",
        '∩' => "\\cap ",
        '∧' => "\\wedge ",
        '∨' => "\\vee ",
        '¬' => "\\neg ",
        '∀' => "\\forall ",
        '∃' => "\\exists ",
        '∅' => "\\emptyset ",
        '⊕' => "\\oplus ",
        '⊗' => "\\otimes ",
        '⊥' => "\\perp ",
        '∥' => "\\parallel ",
        '∠' => "\\angle ",
        '…' => "\\ldots ",
        '→' => "\\rightarrow ",
        '←' => "\\leftarrow ",
        '↔' => "\\leftrightarrow ",
        '⇒' => "\\Rightarrow ",
        '⇐' => "\\Leftarrow ",
        '⇔' => "\\Leftrightarrow ",
        'α' => "\\alpha ",
        'β' => "\\beta ",
        'γ' => "\\gamma ",
        'δ' => "\\delta ",
        'ε' => "\\varepsilon ",
        'ϵ' => "\\epsilon ",
        'ζ' => "\\zeta ",
        'η' => "\\eta ",
        'θ' => "\\theta ",
        'ι' => "\\iota ",
        'κ' => "\\kappa ",
        'λ' => "\\lambda ",
        'μ' => "\\mu ",
        'ν' => "\\nu ",
        'ξ' => "\\xi ",
        'π' => "\\pi ",
        'ρ' => "\\rho ",
        'σ' => "\\sigma ",
        'τ' => "\\tau ",
        'υ' => "\\upsilon ",
        'φ' => "\\phi ",
        'χ' => "\\chi ",
        'ψ' => "\\psi ",
        'ω' => "\\omega ",
        'Γ' => "\\Gamma ",
        'Δ' => "\\Delta ",
        'Θ' => "\\Theta ",
        'Λ' => "\\Lambda ",
        'Ξ' => "\\Xi ",
        'Π' => "\\Pi ",
        'Σ' => "\\Sigma ",
        'Υ' => "\\Upsilon ",
        'Φ' => "\\Phi ",
        'Ψ' => "\\Psi ",
        'Ω' => "\\Omega ",
        '∑' => "\\sum ",
        '∏' => "\\prod ",
        '∫' => "\\int ",
        '√' => "\\sqrt ",
        '′' => "\\prime ",
        '″' => "''",
        '∴' => "\\therefore ",
        '∵' => "\\because ",
        '≪' => "\\ll ",
        '≫' => "\\gg ",
        'ℕ' => "\\mathbb{N}",
        'ℤ' => "\\mathbb{Z}",
        'ℚ' => "\\mathbb{Q}",
        'ℝ' => "\\mathbb{R}",
        'ℂ' => "\\mathbb{C}",
        'ℍ' => "\\mathbb{H}",
        'ℙ' => "\\mathbb{P}",
        'ℓ' => "\\ell ",
        'ℏ' => "\\hbar ",
        '℘' => "\\wp ",
        'ℑ' => "\\Im ",
        'ℜ' => "\\Re ",
        '°' => "^{\\circ}",
        _ => {
            if let Some(styled) = styled_letter(ch) {
                out.push_str(&styled);
            } else {
                out.push(ch);
            }
            return;
        }
    };
    out.push_str(command);
}

fn styled_letter(ch: char) -> Option<String> {
    let code = ch as u32;
    let (cmd, start, origin) = if (0x1D400..=0x1D419).contains(&code) {
        ("\\mathbf", 0x1D400, 'A')
    } else if (0x1D41A..=0x1D433).contains(&code) {
        ("\\mathbf", 0x1D41A, 'a')
    } else if (0x1D434..=0x1D44D).contains(&code) {
        ("", 0x1D434, 'A')
    } else if (0x1D44E..=0x1D467).contains(&code) {
        ("", 0x1D44E, 'a')
    } else if (0x1D468..=0x1D481).contains(&code) {
        ("\\boldsymbol", 0x1D468, 'A')
    } else if (0x1D482..=0x1D49B).contains(&code) {
        ("\\boldsymbol", 0x1D482, 'a')
    } else if (0x1D5A0..=0x1D5B9).contains(&code) {
        ("\\mathsf", 0x1D5A0, 'A')
    } else if (0x1D5BA..=0x1D5D3).contains(&code) {
        ("\\mathsf", 0x1D5BA, 'a')
    } else if (0x1D670..=0x1D689).contains(&code) {
        ("\\mathtt", 0x1D670, 'A')
    } else if (0x1D68A..=0x1D6A3).contains(&code) {
        ("\\mathtt", 0x1D68A, 'a')
    } else {
        return None;
    };
    let letter = char::from_u32(u32::from(origin) + (code - start))?;
    if cmd.is_empty() {
        Some(letter.to_string())
    } else {
        Some(format!("{cmd}{{{letter}}}"))
    }
}

fn text_mode(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '{' | '}' | '$' | '%' | '&' | '#' | '_' | '^' | '~'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
