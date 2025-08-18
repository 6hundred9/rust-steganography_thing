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

    let method = args.method.as_str();
    let filetype = args.filetype.as_str();
    let algorithm = args.algorithm.as_str();
    
    let msg = args.message;

    let ext = Path::new(&args.in_path).extension().and_then(|e| e.to_str()).ok_or("Invalid file extension".to_string()).unwrap().to_string();
    // There HAS to be a better way to do this :sob:
    // God if you can hear me PLEASE let an actually decent rust dev PR on this :sob:
    println!("filetype: {}, alg: {}, method: {}, message: {}", filetype, algorithm, method, msg);
    match method { 
        "find" => {
            match filetype {
                "audio" => {
                    match algorithm {
                        "lsb" => {
                            // WHY DOES -M HIDE END UP HERE I'M GONNA KILL MYSELF
                            let mut bits: Vec<u8> = steg_algorithms::audio::lsb::find_wav(Path::new(&args.in_path)).expect("FUCK YOU AGAIN");

                            // read 32-bit big-endian length
                            let mut len: u32 = 0;
                            for i in 0..32 {
                                len = (len << 1) | (bits[i] as u32);
                            }
                            
                            let mut bytes: Vec<u8> = Vec::with_capacity(len as usize);
                            let start = 32;
                            for byte_idx in 0..(len as usize) {
                                let base = start + byte_idx * 8;
                                let mut b: u8 = 0;
                                for j in 0..8 {
                                    b = (b << 1) | (bits[base + j] & 1);
                                }
                                bytes.push(b);
                            }

                            println!("{}", String::from_utf8(bytes).map_err(|_| "<invalid utf8>".to_string()).expect("This is... dame da stupid..."));
                        }
                        
                        _=>{}
                    }
                }

                "picture" => {
                    match algorithm {
                        "lsb" => {
                            
                        }

                        _=>{}
                    }
                }

                "text" => {

                }

                "video" => {

                }

                _=> {}
            }
        }
        
        "hide" => {
            match filetype {
                "audio" => {
                    match algorithm {
                        "lsb" => {
                            let msg_len = msg.len() as u32;
                            let mut bits: Vec<u8> = Vec::with_capacity(32 + msg.len() * 8);
                            for i in (0..32).rev() { bits.push(((msg_len >> i) & 1) as u8); }
                            for b in msg.bytes() {
                                for i in (0..8).rev() { bits.push(((b >> i) & 1) as u8); }
                            }
                            steg_algorithms::audio::lsb::hide_wav(Path::new(&args.in_path), Path::new(&args.out_path), &bits).expect("FUCK YOU")
                        }

                        _=>{}
                    }
                }

                "picture" => {

                }

                "text" => {

                }

                "video" => {

                }

                _=> {}
            }
        }
        _=> {}
    }
}

// marble pliers