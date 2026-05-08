use binrw::binrw;

#[binrw]
#[derive(PartialEq, Eq, Debug, Clone, Default)]
pub struct Deck {
    #[bw(calc(main.len() as u32 + ex.len() as u32))]
    main_size: u32,
    #[bw(calc(side.len() as u32))]
    side_size: u32,
    #[br(count = main_size)]
    pub main: Vec<u32>, // need reverse
    #[br(count = side_size)]
    pub side: Vec<u32>, // need reverse
    #[br(ignore)]
    pub ex: Vec<u32> // always empty
}

#[binrw]
#[derive(PartialEq, Eq, Debug, Clone, Default)]
pub struct ReplayDeck {
    #[bw(calc(main.len() as u32 + ex.len() as u32))]
    main_size: u32,
    #[br(count = main_size)]
    pub main: Vec<u32>, 
    #[bw(calc(side.len() as u32))]
    side_size: u32,
    #[br(count = side_size)]
    pub side: Vec<u32>,
    #[br(ignore)]
    pub ex: Vec<u32>
}
