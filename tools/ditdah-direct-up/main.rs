fn main() {
    let path = std::env::args().nth(1).expect("usage: ditdah-direct-up <wav>");
    match ditdah::decode_wav_file(&path) {
        Ok(text) => {
            println!("--- DECODED ({} chars) ---", text.len());
            println!("{}", text);
        }
        Err(e) => eprintln!("ERROR: {e}"),
    }
}
