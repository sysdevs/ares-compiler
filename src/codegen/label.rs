//! Block labels and their per-script name generator.

use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Label {
    pub name: String,
}

/// Hands out `${name}_${count}` with a per-name counter; reset per script.
#[derive(Default, Debug)]
pub struct LabelGenerator {
    names: HashMap<String, u32>,
}

impl LabelGenerator {
    pub fn generate(&mut self, name: &str) -> Label {
        let count = self.names.entry(name.to_owned()).or_insert(0);
        let label = Label { name: format!("{name}_{count}") };
        *count += 1;
        label
    }

    pub fn reset(&mut self) {
        self.names.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_per_name_and_resets() {
        let mut g = LabelGenerator::default();
        let names: Vec<_> = ["if_true", "if_true", "if_end", "if_true"].iter().map(|n| g.generate(n).name).collect();
        assert_eq!(names, ["if_true_0", "if_true_1", "if_end_0", "if_true_2"]);
        g.reset();
        assert_eq!(g.generate("if_true").name, "if_true_0");
    }
}
