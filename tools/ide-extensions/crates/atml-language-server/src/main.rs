mod documents;
mod server;

fn main() {
    if let Err(error) = server::run() {
        eprintln!("atml-language-server: {error}");
        std::process::exit(1);
    }
}
