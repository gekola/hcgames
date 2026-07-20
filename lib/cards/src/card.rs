/// rank: 0 = Ace, 1 = 2, …, 12 = King
/// suit: 0 = ♣ (black), 1 = ♦ (red), 2 = ♥ (red), 3 = ♠ (black)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Card {
    pub rank: u8,
    pub suit: u8,
}

impl Card {
    pub fn is_red(self) -> bool {
        self.suit == 1 || self.suit == 2
    }
    /// 0 = black, 1 = red — used for alternating-color tableau checks
    pub fn color_bit(self) -> u8 {
        u8::from(self.is_red())
    }
}

pub fn shuffled_deck() -> Vec<Card> {
    let mut deck: Vec<Card> = (0u8..4)
        .flat_map(|s| (0u8..13).map(move |r| Card { rank: r, suit: s }))
        .collect();
    for i in (1..deck.len()).rev() {
        let j = (macroquad::rand::rand() as usize) % (i + 1);
        deck.swap(i, j);
    }
    deck
}
