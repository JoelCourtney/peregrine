# TODOs for Claude

- [x] Compare the existing submodels that I've already converted with the originals. Until recently I didn't have the ability to specify resource value defaults at the model level. Find resources that have non-trivial defaults in the Java version and make sure they have the same default in the Rust version.
- [ ] A functionality to the model! macro so that when declaring a reactive daemon, you can use `react(*) ...` instead of `react(list, of, resources) ...` to make the daemon react to all resources in the model.
- [ ] The java version of the seis model has a function called updateChannelRates. Implement it as a reactive daemon in the rust version. Split the seis model 
