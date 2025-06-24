#![cfg(target_os = "macos")]
// This is needed for objc
#![allow(unexpected_cfgs)]

pub mod action;
pub mod attribute;
pub mod ui_element;
pub mod util;

use std::cell::{
    Cell,
    RefCell,
};
use std::thread;
use std::time::{
    Duration,
    Instant,
};

use accessibility_sys::AXError;
pub use action::*;
pub use attribute::*;
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use thiserror::Error as TError;
pub use ui_element::*;

#[derive(Debug, TError)]
pub enum Error {
    #[error("element not found")]
    NotFound,
    #[error("accessibility error {0:?}")]
    Ax(AXError),
}

pub trait TreeVisitor {
    fn enter_element(&self, element: &AXUIElement) -> TreeWalkerFlow;
    fn exit_element(&self, element: &AXUIElement);
}

pub struct TreeWalker {
    attr_children: AXAttribute<CFArray<AXUIElement>>,
}

impl Default for TreeWalker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TreeWalkerFlow {
    Continue,
    SkipSubtree,
    Exit,
}

impl TreeWalker {
    pub fn new() -> Self {
        Self {
            attr_children: AXAttribute::children(),
        }
    }

    pub fn walk(&self, root: &AXUIElement, visitor: &dyn TreeVisitor) {
        let _ = self.walk_one(root, visitor);
    }

    fn walk_one(&self, root: &AXUIElement, visitor: &dyn TreeVisitor) -> TreeWalkerFlow {
        let mut flow = visitor.enter_element(root);

        if flow == TreeWalkerFlow::Continue {
            if let Ok(children) = root.attribute(&self.attr_children) {
                for child in children.into_iter() {
                    let child_flow = self.walk_one(&child, visitor);

                    if child_flow == TreeWalkerFlow::Exit {
                        flow = child_flow;
                        break;
                    }
                }
            }
        }

        visitor.exit_element(root);
        flow
    }
}

pub struct ElementFinder {
    root: AXUIElement,
    implicit_wait: Option<Duration>,
    predicate: Box<dyn Fn(&AXUIElement) -> bool>,
    depth: Cell<usize>,
    cached: RefCell<Option<AXUIElement>>,
}

impl ElementFinder {
    pub fn new<F>(root: &AXUIElement, predicate: F, implicit_wait: Option<Duration>) -> Self
    where
        F: 'static + Fn(&AXUIElement) -> bool,
    {
        Self {
            root: root.clone(),
            predicate: Box::new(predicate),
            implicit_wait,
            depth: Cell::new(0),
            cached: RefCell::new(None),
        }
    }

    pub fn find(&self) -> Result<AXUIElement, Error> {
        if let Some(result) = &*self.cached.borrow() {
            return Ok(result.clone());
        }

        let mut deadline = Instant::now();
        let walker = TreeWalker::new();

        if let Some(implicit_wait) = &self.implicit_wait {
            deadline += *implicit_wait;
        }

        loop {
            if let Some(result) = &*self.cached.borrow() {
                return Ok(result.clone());
            }

            walker.walk(&self.root, self);
            let now = Instant::now();

            if now >= deadline {
                return Err(Error::NotFound);
            } else {
                let time_left = deadline.saturating_duration_since(now);
                thread::sleep(std::cmp::min(time_left, Duration::from_millis(250)));
            }
        }
    }

    pub fn reset(&self) {
        self.cached.replace(None);
    }

    pub fn attribute<T: TCFType>(&self, attribute: &AXAttribute<T>) -> Result<T, Error> {
        self.find()?.attribute(attribute)
    }

    pub fn set_attribute<T: TCFType>(&self, attribute: &AXAttribute<T>, value: impl Into<T>) -> Result<(), Error> {
        self.find()?.set_attribute(attribute, value)
    }

    pub fn perform_action(&self, name: &CFString) -> Result<(), Error> {
        self.find()?.perform_action(name)
    }
}

const MAX_DEPTH: usize = 100;

impl TreeVisitor for ElementFinder {
    fn enter_element(&self, element: &AXUIElement) -> TreeWalkerFlow {
        self.depth.set(self.depth.get() + 1);

        if (self.predicate)(element) {
            self.cached.replace(Some(element.clone()));
            return TreeWalkerFlow::Exit;
        }

        if self.depth.get() > MAX_DEPTH {
            TreeWalkerFlow::SkipSubtree
        } else {
            TreeWalkerFlow::Continue
        }
    }

    fn exit_element(&self, _element: &AXUIElement) {
        self.depth.set(self.depth.get() - 1)
    }
}
