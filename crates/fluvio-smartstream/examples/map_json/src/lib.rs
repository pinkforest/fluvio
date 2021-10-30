use serde_json::Value as JsonValue;
use fluvio_smartstream::{smartstream, RecordData, Result};
use fluvio_smartstream::extract::*;

#[smartstream(map)]
pub fn map(record: Record<&[u8], Json<JsonValue>>) -> Result<(Option<RecordData>, RecordData)> {
    let yaml_bytes = serde_yaml::to_vec(&record.value.0)?;
    Ok((record.key.map(|it| it.into()), yaml_bytes.into()))
}