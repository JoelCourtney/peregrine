use peregrine::model;

model! {
    pub Eng {
        pub lander_safe: bool,
        pub apss_safe: bool,
        pub seis_safe: bool,
        pub heat_probe_safe: bool,
        pub ids_safe: bool,
    }
}