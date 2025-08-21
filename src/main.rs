use std::path::PathBuf;
use clap::{Parser, Subcommand};

mod steg_algorithms; // your module

#[derive(Parser, Debug)]
#[command(version, about = "rust-steganography_thing — CLI", long_about = None)]
struct Cli {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Hide a message/file into a carrier
    Hide {
        /// File type (audio, picture, text, video). If omitted will be guessed from input file extension.
        #[arg(short, long)]
        filetype: Option<String>,

        /// Algorithm to use (lsb, ...). If omitted a sensible default will be chosen by filetype.
        #[arg(short, long)]
        algorithm: Option<String>,

        /// Input file path
        #[arg(short = 'i', long)]
        in_path: PathBuf,

        /// Output path (where the stego file will be written)
        #[arg(short = 'o', long)]
        out_path: PathBuf,

        /// Message to hide (for text hiding). If embedding a file, change to reading bytes from a file instead.
        #[arg(long = "msg")]
        message: String,
    },

    /// Find/extract hidden message from a carrier
    Find {
        /// File type (audio, picture, text, video). If omitted will be guessed from input file extension.
        #[arg(short, long)]
        filetype: Option<String>,

        /// Algorithm to use (lsb, ...). If omitted a sensible default will be chosen by filetype.
        #[arg(short, long)]
        algorithm: Option<String>,

        /// Input file path (the stego/carrier)
        #[arg(short = 'i', long)]
        in_path: PathBuf,

        /// Optional output path (for extracted payload). If omitted, prints to stdout.
        #[arg(short = 'o', long)]
        out_path: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    // helper closure to decide filetype (prefer explicit arg, fallback to file extension)
    let detect_filetype = |ft_opt: &Option<String>, in_path: &PathBuf| -> Result<String, String> {
        // if user explicitly passed a filetype, accept a few synonyms and normalize
        if let Some(ft) = ft_opt {
            let ft_l = ft.to_lowercase();
            return match ft_l.as_str() {
                "picture" | "image" | "img" => Ok("picture".to_string()),
                "video" | "movie" => Ok("video".to_string()),
                "audio" | "sound" => Ok("audio".to_string()),
                "text" | "txt" | "string" => Ok("text".to_string()),
                other => Err(format!("Unknown filetype '{}'. Use picture/video/audio/text.", other)),
            };
        }

        // otherwise try to guess from extension
        let ext = in_path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| "Could not detect file extension; provide --filetype".to_string())?
            .to_lowercase();

        match ext.as_str() {
            // images
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tiff" | "tif" |
            "heic" | "heif" | "avif" | "ico" => Ok("picture".to_string()),

            // video
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "flv" | "mpeg" | "mpg" |
            "m4v" | "ogv" | "3gp" => Ok("video".to_string()),

            // audio
            "wav" | "mp3" | "flac" | "ogg" | "opus" | "aac" | "m4a" | "wma" | "alac" => Ok("audio".to_string()),

            // text-ish
            "txt" | "md" | "markdown" | "csv" | "json" | "xml" | "yml" | "yaml" | "html" | "htm" => Ok("text".to_string()),

            other => Err(format!("Unrecognized extension '{}'. Provide --filetype (picture/video/audio/text).", other)),
        }
    };

    match &cli.cmd {
        Command::Hide { filetype, algorithm, in_path, out_path, message } => {
            let ft = match detect_filetype(filetype, in_path) {
                Ok(v) => v,
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            };
            let alg = algorithm.as_deref().unwrap_or_else(|| match ft.as_str() {
                "wav" | "wave" | "audio" => "lsb",
                "picture" => "lsb",
                _ => "lsb", // default fallback
            });

            if cli.verbose {
                println!("hide — filetype: {}, algorithm: {}, in: {:?}, out: {:?}, msg: {}",
                         ft, alg, in_path, out_path, message);
            }

            match ft.as_str() {
                "wav" | "wave" | "audio" => {
                    match alg {
                        "lsb" => {
                            // build bits (32-bit len header + msg bytes, MSB-first)
                            let msg_len = message.len() as u32;
                            let mut bits: Vec<u8> = Vec::with_capacity(32 + message.len() * 8);
                            for i in (0..32).rev() { bits.push(((msg_len >> i) & 1) as u8); }
                            for b in message.bytes() {
                                for i in (0..8).rev() { bits.push(((b >> i) & 1) as u8); }
                            }

                            // call your module
                            if let Err(e) = steg_algorithms::audio::wav::lsb::hide_wav(in_path, out_path, &bits) {
                                eprintln!("hide failed: {}", e);
                                std::process::exit(1);
                            } else if cli.verbose {
                                println!("hide succeeded!");
                            }
                        }
                        other => {
                            eprintln!("Unsupported algorithm '{}' for audio", other);
                            std::process::exit(1);
                        }
                    }
                }

                "picture" => {
                    match alg {
                        "lsb" => {
                            if let Err(e) = steg_algorithms::picture::general::lsb::hide(in_path, message, out_path) {
                                eprintln!("hide failed: {}", e);
                                std::process::exit(1);
                            } else if cli.verbose {
                                println!("hide succeeded!");
                            }
                        }
                        
                        "marker" => {
                            let ext = in_path.extension()
                                .and_then(|e| e.to_str())
                                .ok_or("Invalid file extension")
                                .unwrap();
                            if ext == "jpg" || ext == "jpeg" {
                                if let Err(e) = steg_algorithms::picture::jpg::marker_hijacking::hide(in_path, message, out_path) {
                                    eprintln!("hide failed: {}", e);
                                } else if cli.verbose {
                                    println!("hide succeeded! :3")
                                }
                            } else { 
                                println!("You can only use marker hijacking with jpeg files >:(")
                            }
                        }
                        other => {
                            eprintln!("Unsupported algorithm '{}' for picture", other);
                            std::process::exit(1);
                        }
                    }
                }

                other => {
                    eprintln!("Unsupported filetype '{}'", other);
                    std::process::exit(1);
                }
            }
        }

        Command::Find { filetype, algorithm, in_path, out_path } => {
            let ft = match detect_filetype(filetype, in_path) {
                Ok(v) => v,
                Err(e) => { eprintln!("{}", e); std::process::exit(1); }
            };
            let alg = algorithm.as_deref().unwrap_or_else(|| match ft.as_str() {
                "wav" | "wave" | "audio" => "lsb",
                "png" | "bmp" | "picture" => "lsb",
                _ => "lsb",
            });

            if cli.verbose {
                println!("find — filetype: {}, algorithm: {}, in: {:?}", ft, alg, in_path);
            }

            match ft.as_str() {
                "wav" | "wave" | "audio" => {
                    match alg {
                        "lsb" => {
                            let bits = match steg_algorithms::audio::wav::lsb::find_wav(in_path) {
                                Ok(v) => v,
                                Err(e) => { eprintln!("find failed: {}", e); std::process::exit(1); }
                            };

                            // read 32-bit big-endian length
                            if bits.len() < 32 {
                                eprintln!("Not enough data for header");
                                std::process::exit(1);
                            }
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

                            let output = String::from_utf8(bytes).unwrap_or_else(|_| "<invalid utf8>".to_string());
                            if let Some(out) = out_path {
                                // write to file
                                if let Err(e) = std::fs::write(out, output.as_bytes()) {
                                    eprintln!("Failed to write output file: {}", e);
                                    std::process::exit(1);
                                }
                                if cli.verbose { println!("Wrote decoded output to {:?}", out); }
                            } else {
                                println!("{}", output);
                            }
                        }
                        other => {
                            eprintln!("Unsupported algorithm '{}' for audio", other);
                            std::process::exit(1);
                        }
                    }
                }

                "picture" => {
                    match alg {
                        "lsb" => {
                            let a = steg_algorithms::picture::general::lsb::find(in_path);
                            if let Err(e) = a {
                                eprintln!("find failed: {}", e);
                                std::process::exit(1);
                            } else if cli.verbose {
                                println!("find succeeded, result!");
                            }
                            
                            println!("Result: {}", a.unwrap())
                        }

                        "marker" => {
                            let ext = in_path.extension()
                                .and_then(|e| e.to_str())
                                .ok_or("Invalid file extension")
                                .unwrap();
                            if ext == "jpg" || ext == "jpeg" {
                                let a = steg_algorithms::picture::jpg::marker_hijacking::find(in_path);
                                if let Err(e) = &a {
                                    eprintln!("hide failed: {}", e);
                                } else if cli.verbose {
                                    println!("hide succeeded! :3")
                                }
                                println!("Result: {}", a.unwrap())
                            } else {
                                println!("You can only use marker hijacking with jpeg files >:(")
                            }
                        }
                        
                        other => {
                            eprintln!("Unsupported algorithm '{}' for picture", other);
                            std::process::exit(1);
                        }
                    }
                }

                other => {
                    eprintln!("Unsupported filetype '{}'", other);
                    std::process::exit(1);
                }
            }
        }
    }
}
//bingus