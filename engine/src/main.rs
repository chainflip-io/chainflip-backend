mod mq;

fn main() {
    let message: mq::Message = "hello".as_bytes().to_owned();
    println!("Hello, {:#?}", message);
}
