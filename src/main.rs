use marc::{ Record, Field, Subfield };

fn main() {
    let leader = [1,1,1];
    let fields = vec![
        Field::Control { tag: 001, value: String::from("000001") },
        Field::Control { tag: 005, value: String::from("2026") },
        Field::Standard {
            tag: 200,
            ind: [' ', '1'],
            subfs: vec![
                Subfield { letter: 'a', value: String::from("Mon titre") },
                Subfield { letter: 'e', value: String::from("Complément du titre") },
            ],
        },
        Field::Standard {
            tag: 700,
            ind: [' ', '1'],
            subfs: vec![
                Subfield { letter: 'a', value: String::from("Demians") },
                Subfield { letter: 'b', value: String::from("Frédéric") },
            ],
        },
    ];
    let record = Record {
        leader,
        fields
    };
    println!("{record}");
}
