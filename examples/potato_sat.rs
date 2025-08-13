// #[derive(Init)]
// struct Potato {
//     propellant: Timeline<f32>,
//     battery: Timeline<f32>
// }
//
// struct ThrusterActivity {
//     when: Epoch,
//     duration: Duration,
//     impulse: f32,
// }
//
// impl Activity<Potato> for ThrusterActivity {
//
//     fn act(&self, potato: &mut Potato) {
//         for t in self.when..self.when + self.duration {
//             potato.propellant[t] -= self.impulse;
//             potato.battery[t] -= self.duration.as_millis() as f32 / 1000.0;
//         }
//     }
// }
//
fn main() {
    //     let potato = Potato::init().propellant(50.0).battery(100.0).build();
    //     let g = Graph::new(potato);
    //     let id = g.insert(ThrusterActivity {
    //         when: Epoch::now(),
    //         duration: Duration::from_secs(10),
    //         impulse: 1.0,
    //     });
    //
    //     println!("Hello, world!");
}
