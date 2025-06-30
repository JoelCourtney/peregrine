use peregrine::model;

model! {
    pub Eng {
        pub lander_safe: bool = false,
        pub apss_safe: bool = false,
        pub seis_safe: bool = false,
        pub heat_probe_safe: bool = false,
        pub ids_safe: bool = false,
    }
}
