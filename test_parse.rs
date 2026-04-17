fn main() {
    let content = "0\n";
    let mut lines = content.lines();
    let mut tabs = Vec::new();
    if let Some(first) = lines.next() {
        println!("active: {}", first);
    }
    for line in lines {
        if line.is_empty() {
            tabs.push("None");
        } else {
            tabs.push("Some");
        }
    }
    println!("tabs: {:?}", tabs);
}
