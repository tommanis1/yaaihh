//! Spend: an agent's (or any) tokens with their cost, as one line with the split by type on hover ("$X / N tokens";
//! tokens alone without a price), and the number formats it uses (grouped thousands, K/M/B).

use rune::{El, text};
use rune_kit::Kit;

/// Tokens by type (input without the cached parts) and, when the model has a price, the cost of each type.
#[derive(Clone, Default, Debug)]
pub struct Spend {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
    /// Part of the output.
    pub reasoning: f64,
    /// Dollars for input, cache read, cache write, output.
    pub cost: Option<[f64; 4]>,
}

impl Spend {
    pub fn tokens(&self) -> f64 {
        self.input + self.cache_read + self.cache_write + self.output
    }

    /// "$1.23 / 214.2K", or the tokens alone when there is no price.
    pub fn label(&self) -> String {
        match self.cost {
            Some(c) => format!("${} / {}", num(c.iter().sum(), 2, 2), compact(self.tokens())),
            None => compact(self.tokens()),
        }
    }

    /// Each type's tokens and dollars, one per line.
    pub fn breakdown(&self) -> String {
        let types = [("Input (uncached)", self.input), ("Cache read", self.cache_read), ("Cache write", self.cache_write), ("Output", self.output)];
        let dollars = |i: usize| self.cost.map_or(String::new(), |c| format!("  ${}", num(c[i], 2, 4)));
        let mut lines: Vec<String> = types.iter().enumerate().map(|(i, (l, t))| format!("{l}: {} tokens{}", num(*t, 0, 0), dollars(i))).collect();
        if self.reasoning > 0.0 {
            lines[3].push_str(&format!(" (reasoning {})", num(self.reasoning, 0, 0)));
        }
        lines.push(match self.cost {
            Some(c) => format!("Total: {} tokens  ${}", num(self.tokens(), 0, 0), num(c.iter().sum(), 2, 4)),
            None => format!("Total: {} tokens (no price for this model)", num(self.tokens(), 0, 0)),
        });
        lines.join("\n")
    }
}

/// The spend as one line, its split on hover.
pub fn spend(k: &Kit, s: &Spend) -> El {
    text(s.label()).font(k.ty.small_medium).color(k.c.text_base).truncate().help(s.breakdown())
}

/// V with grouped thousands and MIN to MAX decimals.
pub fn num(v: f64, min: usize, max: usize) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{:.*}", max, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut frac = frac.trim_end_matches('0').to_string();
    while frac.len() < min {
        frac.push('0');
    }
    let grouped = count(int.parse::<f64>().unwrap_or(0.0));
    let zero = int.chars().all(|c| c == '0') && frac.chars().all(|c| c == '0');
    format!("{}{grouped}{}{frac}", if v < 0.0 && !zero { "-" } else { "" }, if frac.is_empty() { "" } else { "." })
}

/// A count in K, M or B with one decimal.
pub fn compact(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let a = v.abs();
    let thousands = (a / 1e3 * 10.0).round() / 10.0;
    let millions = (a / 1e6 * 10.0).round() / 10.0;
    let (div, unit) = if a >= 1e9 || millions >= 1000.0 {
        (1e9, "B")
    } else if a >= 1e6 || thousands >= 1000.0 {
        (1e6, "M")
    } else if a >= 1e3 {
        (1e3, "K")
    } else {
        (1.0, "")
    };
    format!("{}{unit}", num(v / div, 0, if div == 1.0 { 0 } else { 1 }))
}

/// A count with thousands separators.
pub fn count(n: f64) -> String {
    let n = n.round() as i64;
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}
