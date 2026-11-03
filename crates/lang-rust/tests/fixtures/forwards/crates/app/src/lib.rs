mod helpers;
mod local;

use front::builder::Factory;
use front::tools::Spade;
use front::Thing;

pub fn run(thing: &dyn Thing, spade: Spade, factory: Factory) {
    thing.act();
    drop((spade, factory));
}
