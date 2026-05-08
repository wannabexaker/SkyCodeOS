pub mod apply;
pub mod diff;
pub mod filesystem;
pub mod process;
pub mod rollback;
pub mod verify;

use std::fmt::Debug;

pub trait Tool {
    type Input;
    type Output;
    type Error: std::error::Error + Send + Sync + 'static;

    fn name(&self) -> &'static str;
    fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;
}

pub trait ToolInput: Debug + Send + Sync {}

impl<T> ToolInput for T where T: Debug + Send + Sync {}
