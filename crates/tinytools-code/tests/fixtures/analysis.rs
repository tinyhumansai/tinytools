//! Shared lexical-analysis and benchmark fixture.

/// A named value.
pub struct Record {
    pub value: i32,
}

impl Record {
    pub fn increment(&mut self) {
        self.value += 1;
        self.value += 2;
    }
}

pub mod nested {
    pub fn calculate(value: i32) -> i32 {
        let doubled = value * 2;
        let adjusted = doubled + 1;
        adjusted
    }
}
