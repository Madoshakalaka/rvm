use tracing::error;

pub fn eprint_error_chain(e: impl std::error::Error) {
    if let Some(source) = e.source() {
        eprint_error_chain(source);
        println!("↓");
    }
    eprintln!("{:}", e);
}


pub fn _report_error_chain(e: impl std::error::Error, is_source:bool){
    if let Some(source) = e.source(){
        _report_error_chain(source, true);
    }
    match is_source{
        true => {
            error!("{}\tThis is the cause of the next error.", e);
        }
        false => {
            error!("{}", e);
        }
    }

}


pub fn report_error_chain(e: impl std::error::Error){
    _report_error_chain(e, false);
}
