

pub fn eprint_error_chain(e: impl std::error::Error){
    if let Some(source) = e.source(){
        eprint_error_chain(source);
        println!("↓");
    }
    eprintln!("{:}", e);
}