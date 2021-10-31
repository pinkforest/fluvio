use quote::quote;
use proc_macro2::TokenStream;
use crate::SmartModuleFn;

pub fn generate_aggregate_smartmodule(func: &SmartModuleFn, has_params: bool) -> TokenStream {
    let user_code = &func.func;
    let user_fn = &func.name;

    let params_parsing = if has_params {
        quote!(
            use std::convert::TryInto;

            let params = match smartmodule_input.base.params.try_into(){
                Ok(params) => params,
                Err(err) => return SmartModuleInternalError::ParsingExtraParams as i32,
            };

        )
    } else {
        quote!()
    };

    let function_call = if has_params {
        quote!(
            super:: #user_fn(acc_data, arg, &params)
        )
    } else {
        quote!(
            super:: #user_fn(acc_data, arg)
        )
    };

    quote! {
        #user_code

        mod __system {
            #[no_mangle]
            #[allow(clippy::missing_safety_doc)]
            pub unsafe fn aggregate(ptr: &mut u8, len: usize, version: i16) -> i32 {
                use fluvio_smartmodule::dataplane::smartmodule::{
                    SmartModuleAggregateInput, SmartModuleInternalError,
                    SmartModuleRuntimeError, SmartModuleKind, SmartModuleOutput,SmartModuleAggregateOutput
                };
                use fluvio_smartmodule::dataplane::core::{Encoder, Decoder, bytes::Bytes};
                use fluvio_smartmodule::dataplane::record::{Record, RecordData};
                use fluvio_smartmodule::extract::{FromRecord, FromBytes};
                use fluvio_smartmodule::Error;

                extern "C" {
                    fn copy_records(putr: i32, len: i32);
                }

                let input_data = Vec::from_raw_parts(ptr, len, len);
                let mut smartmodule_input = SmartModuleAggregateInput::default();
                // 13 is version for aggregate
                if let Err(_err) = Decoder::decode(&mut smartmodule_input, &mut std::io::Cursor::new(input_data), version) {
                    return SmartModuleInternalError::DecodingBaseInput as i32;
                }

                let mut accumulator = smartmodule_input.accumulator;

                #params_parsing

                let records_input = smartmodule_input.base.record_data;
                let mut records: Vec<Record> = vec![];
                if let Err(_err) = Decoder::decode(&mut records, &mut std::io::Cursor::new(records_input), version) {
                    return SmartModuleInternalError::DecodingRecords as i32;
                };

                // PROCESSING
                let mut output = SmartModuleAggregateOutput {
                    base: SmartModuleOutput {
                        successes: Vec::with_capacity(records.len()),
                        error: None,
                    },
                    accumulator: Vec::new()
                };

                for mut record in records.into_iter() {
                    let acc_bytes = Bytes::copy_from_slice(&accumulator);
                    let acc_data = match FromBytes::from_bytes(&acc_bytes) {
                        Ok(inner) => inner,
                        Err(err) => {
                            let error = SmartModuleRuntimeError::new(
                                &record,
                                smartmodule_input.base.base_offset.clone(),
                                SmartModuleKind::Aggregate,
                                Error::from(err),
                            );
                            output.base.error = Some(error);
                            continue;
                        }
                    };

                    let arg = match FromRecord::from_record(&record) {
                        Ok(inner) => inner,
                        Err(err) => {
                            let error = SmartModuleRuntimeError::new(
                                &record,
                                smartmodule_input.base.base_offset.clone(),
                                SmartModuleKind::Aggregate,
                                Error::from(err),
                            );
                            output.base.error = Some(error);
                            continue;
                        }
                    };

                    let result = #function_call;
                    match result {
                        Ok(value) => {
                            accumulator = Vec::from(value.as_ref());
                            output.accumulator = accumulator.clone();
                            record.value = RecordData::from(accumulator.clone());
                            output.base.successes.push(record);
                        }
                        Err(err) => {
                            let error = SmartModuleRuntimeError::new(
                                &record,
                                smartmodule_input.base.base_offset,
                                SmartModuleKind::Aggregate,
                                err,
                            );
                            output.base.error = Some(error);
                            break;
                        }
                    }
                }

                let output_len = output.base.successes.len() as i32;

                // ENCODING
                let mut out = vec![];
                if let Err(_) = Encoder::encode(&mut output, &mut out, version) {
                    return SmartModuleInternalError::EncodingOutput as i32;
                }

                let out_len = out.len();
                let ptr = out.as_mut_ptr();
                std::mem::forget(out);
                copy_records(ptr as i32, out_len as i32);
                output_len
            }
        }
    }
}
