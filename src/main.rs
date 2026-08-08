use marc::{ Record, Field, Subfield };

fn main() {
    let leader = [1,1,1];
    let fields = vec![
        Field::Control(001, String::from("000001")),
        Field::Control(005, String::from("2026")),
        Field::Standard(200, [' ', '1'],
            vec![
                Subfield('a', String::from("Mon titre")),
                Subfield('e', String::from("Complément du titre")),
            ]),
        Field::Standard(700, [' ', '1'],
            vec![
                Subfield('a', "Demians".to_string()),
                Subfield('b', String::from("Frédéric")),
            ]),
    ];
    let mut record = Record {
        leader,
        fields
    };

    let mut add_field = |values: Vec<&str>| -> () {
        let tag: u16 = values[0].parse().expect("Tag invalude");
        println!("val: {tag}");
        record.add(Field::Standard(214, [' ', '1'],
            vec![Subfield('a', "Paris".to_string())] ));
        ()
    };
    add_field(vec!["200", "1 ", "a", "kjdsfksdfjk"]);

    println!("{}", record);
}

