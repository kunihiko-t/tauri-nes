use std::collections::HashSet;

pub struct Debugger {
    breakpoints: HashSet<u16>, // ブレークポイントを保持するセット
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            breakpoints: HashSet::new(),
        }
    }

    pub fn add_breakpoint(&mut self, addr: u16) {
        self.breakpoints.insert(addr);
    }

    pub fn remove_breakpoint(&mut self, addr: u16) {
        self.breakpoints.remove(&addr);
    }

    // 実行中のアドレスがブレークポイントに達したかをチェック
    pub fn check_breakpoint(&self, pc: u16) -> bool {
        self.breakpoints.contains(&pc)
    }
}
