fn main(){
    while let Ok(_) = Some(1){
        println!("should never")
    }
    println!("should once")
}