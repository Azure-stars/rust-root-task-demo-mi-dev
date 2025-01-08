macro_rules! register_block_driver {
    ($driver_type:ty, $device_type:ty) => {
        /// The unified type of the NIC devices.
        #[cfg(not(feature = "dyn"))]
        pub type AxBlockDevice = $device_type;
    };
}
