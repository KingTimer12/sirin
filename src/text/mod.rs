pub struct SourceText {
    text: String
}

impl SourceText {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn line_index(&self, pos: usize) -> usize {
        self.text[..pos].lines().count()
    }

    pub fn get_line(&self, index: usize) -> &str {
        self.text.lines().nth(index).unwrap()
    }

    pub fn line_start(&self, index: usize) -> usize {
        self.text.lines().take(index).map(|line| line.len() + 1).sum()
    }
}