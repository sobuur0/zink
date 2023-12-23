//! Code generators
//!
//! - CONSTRUCTOR
//! - DISPATCHER
//! - FUNCTION
//! - CODE

mod code;
mod constructor;
mod dispatcher;
mod function;

pub use self::{
    code::{Code, ExtFunc},
    constructor::Constructor,
    dispatcher::Dispatcher,
    function::Function,
};
