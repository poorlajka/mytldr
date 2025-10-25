use pulldown_cmark::{ CodeBlockKind, CowStr, Event, Parser, Tag, TagEnd };
use crossterm::{
    execute, style::{Attribute, Color, Print, ResetColor, SetBackgroundColor, SetColors, SetForegroundColor, Stylize}, Command, QueueableCommand
};
use std::io::{self, stdout, Stdout, Write};
use std::fmt;

pub struct MdRenderer {
    out: Stdout,
    context_stack: Vec<Context>,
    margin_left: String,
}

enum Context {
    CodeBlock,
    Paragraph,
    List(Option<u64>),
    ListItem,
    Heading(u8),
    BlockQuotes,
}

impl MdRenderer {
    pub fn new() -> Self {
        Self {
            out: stdout(),
            context_stack: Vec::new(),
            margin_left: String::from(""),
        }
    }

    fn push_context(&mut self, context: Context) {
        self.context_stack.push(context);
    }

    fn pop_context(&mut self) -> Option<Context> {
        self.context_stack.pop()
    }

    fn peek_context(&self) -> Option<&Context> {
        self.context_stack.last()
    }

    pub fn render_md(&mut self, md: &str) ->  io::Result<()> {
        println!("");

        let md_parser = Parser::new(md);
        for event in md_parser {
            match event {
                Event::Start(tag) => self.process_start_tag(&tag),
                Event::End(tag_end) => self.process_end_tag(&tag_end)?,
                Event::Text(cow_str) => self.render_text(&cow_str)?,
                Event::Code(cow_str) => self.render_code(&cow_str)?,
                Event::TaskListMarker(is_marked) => self.render_task_list_marker(is_marked)?,
                Event::SoftBreak => self.render_soft_break()?,
                Event::HardBreak => self.render_hard_break()?,
                Event::Rule => self.render_rule()?,
                Event::InlineMath(_) 
                | Event::DisplayMath(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::FootnoteReference(_) => {},
            }
        }

        Ok(())
    }

    fn process_start_tag(&mut self, tag: &Tag) {

        let context = match tag {
            Tag::Paragraph => Some(Context::Paragraph),
            Tag::CodeBlock(_) => {
                print!("{} ", self.margin_left);
                Some(Context::CodeBlock)
            },
            Tag::Heading { level, .. } => Some(Context::Heading(*level as u8)),
            Tag::BlockQuote(_) => {
                print!("|");
                Some(Context::BlockQuotes)
            }
            Tag::List(starting_at) => {
                println!("");
                self.margin_left = String::from("    ");
                Some(Context::List(*starting_at))
            },
            Tag::Item => {
                print!("{}* ", self.margin_left);
                Some(Context::ListItem)
            },
            Tag::HtmlBlock
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList 
            | Tag::DefinitionListTitle 
            | Tag::DefinitionListDefinition 
            | Tag::Table(_) 
            | Tag::TableHead 
            | Tag::TableRow 
            | Tag::TableCell 
            | Tag::Emphasis 
            | Tag::Strong 
            | Tag::Strikethrough 
            | Tag::Superscript 
            | Tag::Subscript 
            | Tag::MetadataBlock(_)
            | Tag::Link { .. } 
            | Tag::Image { .. } => None,
        };

        if let Some(context) = context {
            self.context_stack.push(context);
        }
    }

    fn process_end_tag(&mut self, tag_end: &TagEnd) -> io::Result<()> {

        match tag_end {
            TagEnd::Paragraph => {},
            TagEnd::Heading(level) => {
                println!("");
                println!("");
            },
            TagEnd::BlockQuote(kind) => {
                println!("");
                println!("");
            },
            TagEnd::CodeBlock => {
                println!("");
            }, 
            TagEnd::HtmlBlock => {},
            TagEnd::List(starting_from) => {
                self.margin_left = String::from("");
            },
            TagEnd::Item => {
                println!("");
                println!("");
            },
            TagEnd::FootnoteDefinition => {},
            TagEnd::DefinitionList => {},
            TagEnd::DefinitionListTitle => {},
            TagEnd::DefinitionListDefinition => {},
            TagEnd::Table => {},
            TagEnd::TableHead => {},
            TagEnd::TableRow => {},
            TagEnd::TableCell => {},
            TagEnd::Emphasis => {},
            TagEnd::Strong => {},
            TagEnd::Strikethrough => {},
            TagEnd::Superscript => {},
            TagEnd::Subscript => {},
            TagEnd::Link => {},
            TagEnd::Image => {},
            TagEnd::MetadataBlock(kind) => {},
        }

        Ok(())
    }

    fn render_text(&mut self, text: &str) -> io::Result<()> {
        match self.peek_context() {
           Some(Context::BlockQuotes) => {

           } 
            _ => {},
        }
        print!("{}", text);
        Ok(())
    }
    
    fn render_code(&mut self, code: &str) -> io::Result<()> {
        let bg_code = self.set_text_bg_color(&format!(" {code} "), &Color::Grey);
        let fg_code = self.set_text_fg_color(&bg_code, &Color::Black);
        print!("     {}", fg_code);
        //This will work
        //if let None = self.peek_context()
        Ok(())
    }

    fn render_task_list_marker(&mut self, is_checked: bool) -> io::Result<()> {
        Ok(())
    }

    fn render_soft_break(&mut self) -> io::Result<()> {
        println!(" ");
        
        Ok(())
    }

    fn render_hard_break(&mut self) -> io::Result<()> {
        println!("");
        Ok(())
    }

    fn render_rule(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn set_text_bg_color(&self, text: &str, bg: &Color) -> String {
        text.on(*bg).to_string()
    }

    fn set_text_fg_color(&self, text: &str, fg: &Color) -> String {
        text.with(*fg).to_string()
    }

    fn set_term_attribute(attribute: &Attribute) -> io::Result<()> {
        Ok(())
    }

}

