use std::ops::Range;

use edgedoc_limit::Budget;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{OpcError, OpcResult};

#[derive(Debug, Clone)]
pub enum Node {
    El(El),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct El {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub span: Range<usize>,
}

impl El {
    pub fn attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| local_name(name) == local)
            .map(|(_, value)| value.as_str())
    }

    pub fn children<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.child_els().filter(move |el| el.name == local)
    }

    pub fn child_els(&self) -> impl Iterator<Item = &El> {
        self.children.iter().filter_map(|node| match node {
            Node::El(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for node in &self.children {
            if let Node::Text(text) = node {
                out.push_str(text);
            }
        }
        out
    }

    pub fn raw<'a>(&self, source: &'a [u8]) -> &'a str {
        let end = self.span.end.min(source.len());
        let start = self.span.start.min(end);
        std::str::from_utf8(&source[start..end]).unwrap_or("")
    }
}

pub fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub fn parse(bytes: &[u8], budget: &Budget) -> OpcResult<El> {
    if contains_doctype(bytes) {
        return Err(OpcError::new(
            "xml.malformed",
            "The part contains a DTD. Remove the DTD and send the part again.",
        ));
    }
    let mut reader = Reader::from_reader(bytes);
    reader.check_end_names(true);
    let mut buf = Vec::new();
    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;
    let mut elements = 0usize;
    loop {
        let start_pos = reader.buffer_position();
        let event = reader.read_event_into(&mut buf).map_err(|err| {
            OpcError::new(
                "xml.malformed",
                format!("The part is not well-formed XML. {err}"),
            )
        })?;
        match event {
            Event::Start(start) => {
                elements += 1;
                budget.elements(elements).map_err(map_limit)?;
                budget.depth(stack.len() + 1).map_err(map_limit)?;
                stack.push(read_el(bytes, &start, start_pos, budget)?);
            }
            Event::Empty(start) => {
                elements += 1;
                budget.elements(elements).map_err(map_limit)?;
                budget.depth(stack.len() + 1).map_err(map_limit)?;
                let mut el = read_el(bytes, &start, start_pos, budget)?;
                el.span.end = reader.buffer_position();
                push_child(&mut stack, &mut root, Node::El(el))?;
            }
            Event::End(_) => {
                let mut el = stack.pop().ok_or_else(|| {
                    OpcError::new("xml.malformed", "The part has an extra end tag.")
                })?;
                el.span.end = reader.buffer_position();
                push_child(&mut stack, &mut root, Node::El(el))?;
            }
            Event::Text(text) => {
                let value = text
                    .unescape()
                    .map_err(|err| OpcError::new("xml.malformed", err.to_string()))?
                    .into_owned();
                if !value.is_empty() {
                    push_child(&mut stack, &mut root, Node::Text(value))?;
                }
            }
            Event::CData(text) => {
                let value = String::from_utf8_lossy(&text).into_owned();
                push_child(&mut stack, &mut root, Node::Text(value))?;
            }
            Event::DocType(_) => {
                return Err(OpcError::new(
                    "xml.malformed",
                    "The part contains a DTD. Remove the DTD and send the part again.",
                ));
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    root.ok_or_else(|| OpcError::new("xml.malformed", "The part has no root element."))
}

fn read_el(
    source: &[u8],
    start: &quick_xml::events::BytesStart<'_>,
    start_pos: usize,
    budget: &Budget,
) -> OpcResult<El> {
    let owned_name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|err| OpcError::new("xml.malformed", err.to_string()))?;
        budget.attrs(attrs.len() + 1).map_err(map_limit)?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = attr
            .unescape_value()
            .map_err(|err| OpcError::new("xml.malformed", err.to_string()))?
            .into_owned();
        attrs.push((local_name(&key).to_string(), value));
    }
    let _ = source;
    Ok(El {
        name: local_name(&owned_name).to_string(),
        attrs,
        children: Vec::new(),
        span: start_pos..start_pos,
    })
}

fn push_child(stack: &mut [El], root: &mut Option<El>, node: Node) -> OpcResult<()> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
        Ok(())
    } else if root.is_none() {
        match node {
            Node::El(el) => {
                *root = Some(el);
                Ok(())
            }
            Node::Text(_) => Ok(()),
        }
    } else {
        Err(OpcError::new(
            "xml.malformed",
            "The part has more than one root element.",
        ))
    }
}

fn contains_doctype(bytes: &[u8]) -> bool {
    let lower = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    lower.contains("<!doctype")
}

fn map_limit(err: edgedoc_limit::LimitError) -> OpcError {
    OpcError::new(err.code, err.message)
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}
