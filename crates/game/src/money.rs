use std::ops::AddAssign;
use std::ops::SubAssign;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Coin {
    Nickel,
    Dime,
    Quarter,
    Loonie,
    Toonie,
}

impl Coin {
    pub fn as_cents(&self) -> u32 {
        match self {
            Coin::Nickel => 5,
            Coin::Dime => 10,
            Coin::Quarter => 25,
            Coin::Loonie => 100,
            Coin::Toonie => 200,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Default)]
pub struct MoneyJar(u32);
impl MoneyJar {
    pub fn new(cents: u32) -> Self {
        MoneyJar(cents)
    }
}

impl AddAssign for MoneyJar {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for MoneyJar {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl From<Vec<Coin>> for MoneyJar {
    fn from(value: Vec<Coin>) -> Self {
        MoneyJar(value.iter().map(|c| c.as_cents()).sum())
    }
}

impl MoneyJar {
    pub fn total_cents(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for MoneyJar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} cents", self.total_cents()))
    }
}
