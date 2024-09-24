// Trait to handle serialization and deserialization of callback data.
pub trait CallbackDataHandler: Sized {
    // Serializes the enum variant into a callback data string with the given prefix.
    fn to_callback_data(&self, prefix: &str) -> String;

    // Deserializes the callback data string into the enum variant using the given prefix.
    fn from_callback_data(data: &str, prefix: &str) -> Option<Self>;
}