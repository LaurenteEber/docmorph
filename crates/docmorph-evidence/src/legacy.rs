use super::{Arguments, execute_legacy};

pub(super) fn execute(command: Vec<String>, arguments: Arguments) -> Result<(), String> {
    execute_legacy(command, arguments)
}
