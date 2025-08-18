use std::path::Path;
use clap::builder::Str;
use clap::Parser;

mod steg_algorithms;
// damn was :sob: :evil_party_popper:

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Filetype for steno, supported: audio, picture, text, video
    #[arg(short, long)]
    filetype : String,
    
    /// This will highly vary depending on filetype --
    /// audio: lsb (.waw) --
    /// picture: lsb (.png) --
    /// text: None --
    /// video: None --
    #[arg(short, long)]
    algorithm : String,
    
    /// hide/find
    #[arg(short, long)]
    method : String,
    
    /// Path for input file
    #[arg(short, long)]
    in_path : String,
    
    /// Path for result
    #[arg(short, long)]
    out_path : String,
    
    /// Go crazy, just make sure it'll fit in the file
    #[arg(long="msg")]
    message : String,

    #[arg(long, default_value = "false")]
    verbose : bool,
}



fn main() {
    let find_str = "find".to_string();
    let hide_str = "hide".to_string();
    let audio_str = "audio".to_string();
    let picture_str = "picture".to_string();
    let text_str = "text".to_string();
    let video_str = "video".to_string();
    let lsb_str = "lsb".to_string();
    // wilted flower emoji pensive emoji
    
    
    let args = Args::parse();
    // spaghetti
    // better than 500 if statements tho :pensive:

    let msg = args.message;

    let ext = Path::new(&args.in_path).extension().and_then(|e| e.to_str()).ok_or("Invalid file extension".to_string()).unwrap().to_string();
    // There HAS to be a better way to do this :sob:
    // God if you can hear me PLEASE let an actually decent rust dev PR on this :sob:
    match args.method { 
        find_str => {
            match args.filetype {
                audio_str => {
                    match args.algorithm {
                        lsb_str => {
                            let msg_len = msg.len() as u32;
                            let mut bits: Vec<u8> = Vec::with_capacity(32 + msg.len() * 8);
                            for i in (0..32).rev() { bits.push(((msg_len >> i) & 1) as u8); }
                            for b in msg.bytes() {
                                for i in (0..8).rev() { bits.push(((b >> i) & 1) as u8); }
                            }
                            steg_algorithms::audio::lsb::hide_wav(Path::new(&args.in_path), Path::new(&args.out_path), &bits).expect("FUCK YOU")
                        }
                    }
                }

                picture_str => {
                    match args.algorithm {
                        lsb_str => {
                            
                        }
                    }
                }

                text_str => {

                }

                video_str => {

                }

                _=> {}
            }
        }
        
        hide_str => {
            match args.filetype {
                audio_str => {

                }

                picture_str => {

                }

                text_str => {

                }

                video_str => {

                }

                _=> {}
            }
        }
        _=> {}
    }
}

// marble pliers