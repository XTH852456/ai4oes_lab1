use crate::{print, println};

#[derive(Copy, Clone)]
pub enum Shape {
    O,
    S,
}

const PIECES: usize = 7;

const O_TEMPLATE: &[&str] = &[
    "........AAAABBBB........",
    "......AAAAAABBBBBB......",
    "....CCCC........DDDD....",
    "...CCCCC........DDDDD...",
    "..CCCC............DDDD..",
    "..CCCC............DDDD..",
    "..EEEE............FFFF..",
    "..EEEE............FFFF..",
    "..EEEE............FFFF..",
    "...EEEEE........FFFFF...",
    "....GGGG........GGGG....",
    "......GGGGGGGGGGGG......",
    "........GGGGGGGG........",
];

const S_TEMPLATE: &[&str] = &[
    ".......AAAABBBB.........",
    ".....AAAAAABBBBBB.......",
    "...CCCCCC...DDDDDD......",
    "..CCCCCCC.....DDDDD.....",
    "..CCCCC..................",
    "...CCCC.................",
    ".....EEEEEEEEE..........",
    ".......EEEEEEEE.........",
    "..............FFFF......",
    "...............FFFFF....",
    "................FFFFF...",
    "........GGGGGGGFFFF.....",
    "......GGGGGGGGFFFF......",
    "....GGGGGG..............",
    "......GGGG................",
];

pub fn render(shape: Shape, stage: usize) {
    let stage = stage.min(PIECES - 1);
    let (name, template) = match shape {
        Shape::O => ("O", O_TEMPLATE),
        Shape::S => ("S", S_TEMPLATE),
    };
    let piece = (b'A' + stage as u8) as char;

    println!("================ Tangram {name} Batch {}/{} ================", stage + 1, PIECES);
    println!("current piece: {piece}");
    println!("pieces: A B C D E F G");
    println!();

    for &line in template {
        render_line(line, stage);
    }

    println!();
}

fn render_line(line: &str, stage: usize) {
    for byte in line.bytes() {
        match byte {
            b'.' => print!(" "),
            b'A'..=b'G' if (byte - b'A') as usize <= stage => print!("{}", byte as char),
            b'A'..=b'G' => print!(" "),
            _ => print!("{}", byte as char),
        }
    }
    println!();
}