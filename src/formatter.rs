#![allow(unused)]

use std::io::Write;

use crate::error::Result;
use comrak::{format_commonmark, nodes::AstNode, parse_document, Arena, ComrakOptions};
use log::warn;

pub type FormatFunction<'a> = fn(&'a AstNode<'a>) -> Result<()>;
pub type FormatFunctionWithArgs<'a, Arg> = fn(&'a AstNode<'a>, &'a Arg) -> Result<()>;

pub struct Formatter<'a> {
    source: Option<&'a str>,
    root: Option<&'a AstNode<'a>>,
    arena: Arena<AstNode<'a>>,
    options: ComrakOptions,
}

impl<'a> Default for Formatter<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Formatter<'a> {
    pub fn new() -> Self {
        let arena = Arena::new();
        let mut options = ComrakOptions::default();

        options.extension.superscript = true;
        options.extension.table = true;

        options.parse.default_info_string = Some("text".into());

        Self {
            source: Default::default(),
            root: None,
            arena,
            options,
        }
    }

    pub fn parse(&'a mut self, markdown: &'a str) -> &Self {
        let root = parse_document(&self.arena, markdown, &self.options);
        self.source = Some(markdown);
        self.root = Some(root);
        self
    }

    pub fn format(&self, func: FormatFunction<'a>) -> &Self {
        if let Some(root) = self.root {
            Self::iter_nodes(root, func);
            return self;
        }
        warn!("Can not format before parse.");
        self
    }

    pub fn format_with_args<Args>(
        &self,
        f: FormatFunctionWithArgs<'a, Args>,
        args: &'a Args,
    ) -> &Self {
        if let Some(root) = self.root {
            Self::iter_nodes_with_args(root, f, args);
            return self;
        }

        warn!("Can not format before parse.");
        self
    }

    fn iter_nodes<'n>(node: &'n AstNode<'n>, f: FormatFunction<'n>) {
        f(node).ok();
        for c in node.children() {
            Self::iter_nodes(c, f);
        }
    }

    fn iter_nodes_with_args<'n, Args>(
        node: &'n AstNode<'n>,
        f: FormatFunctionWithArgs<'n, Args>,
        args: &'n Args,
    ) {
        f(node, args).ok();
        for n in node.children() {
            Self::iter_nodes_with_args(n, f, args);
        }
    }

    pub fn write_to(&self, file: &mut impl Write) {
        let mut file = file;
        if let Some(root) = self.root {
            format_commonmark(root, &self.options, &mut file).ok();
            return;
        }

        warn!("Can not format before parse.");
    }
}
