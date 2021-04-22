/// Start witnessing the state chain
pub fn start() {
    println!("Start the state chain witness");
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn testing_stuff() {
        start();
    }
}
