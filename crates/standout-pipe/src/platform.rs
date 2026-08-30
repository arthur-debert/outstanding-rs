use crate::pipe::SimplePipe;

#[cfg(target_os = "macos")]
pub fn clipboard() -> Option<SimplePipe> {
    Some(SimplePipe::new("pbcopy").consume())
}

#[cfg(target_os = "linux")]
pub fn clipboard() -> Option<SimplePipe> {
    Some(SimplePipe::new("xclip -selection clipboard").consume())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn clipboard() -> Option<SimplePipe> {
    None
}
