//! Recording: every call a driver makes to its device, kept as transcript events. The recorder sits
//! between [`crate::peripheral::Io`] and the device and only writes down what goes through: it sends
//! nothing of its own, and a call that failed is not kept.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use super::{Event, Op};
use crate::error::Result;
use crate::peripheral::Hid;

/// Most events kept for one reading: a reading of a device's whole memory is cut there.
const MAX_EVENTS: usize = 4000;

#[derive(Default)]
struct Tape {
    events: Vec<Event>,
    cut: bool,
}

/// The events since the last [`Recording::take`], shared by the collections of one device.
#[derive(Clone, Default)]
pub struct Recording(Arc<Mutex<Tape>>);

impl Recording {
    fn push(&self, event: Event) {
        let mut tape = self.0.lock().unwrap();
        // The descriptor is asked for at every reading by some drivers: once is enough.
        if event.op == Op::Descriptor
            && tape.events.iter().any(|e| e.op == Op::Descriptor && e.collection == event.collection)
        {
            return;
        }
        if tape.events.len() < MAX_EVENTS {
            tape.events.push(event);
        } else {
            tape.cut = true;
        }
    }

    /// The events since the last call, with a mark at the end if some were left out.
    pub fn take(&self) -> Vec<Event> {
        let mut tape = self.0.lock().unwrap();
        let mut events = std::mem::take(&mut tape.events);
        if std::mem::take(&mut tape.cut) {
            events.push(Event::mark(&format!("cut after {MAX_EVENTS} events")));
        }
        events
    }

    /// `hid`, with every call kept here under `collection`.
    pub fn wrap(&self, hid: Box<dyn Hid>, collection: &'static str) -> Box<dyn Hid> {
        Box::new(Recorder { inner: hid, tape: self.clone(), collection })
    }
}

struct Recorder {
    inner: Box<dyn Hid>,
    tape: Recording,
    collection: &'static str,
}

impl Recorder {
    fn keep(&self, op: Op, report_id: Option<u8>, bytes: &[u8]) {
        self.tape.push(Event::new(op, self.collection, report_id, bytes));
    }
}

impl Hid for Recorder {
    fn write(&self, data: &[u8]) -> Result<usize> {
        let n = self.inner.write(data)?;
        self.keep(Op::Write, None, data);
        Ok(n)
    }
    fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(buf, timeout_ms)?;
        if n > 0 {
            self.keep(Op::Read, None, &buf[..n.min(buf.len())]);
        }
        Ok(n)
    }
    fn send_feature(&self, data: &[u8]) -> Result<()> {
        self.inner.send_feature(data)?;
        self.keep(Op::FeatureSet, None, data);
        Ok(())
    }
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        let id = buf.first().copied().unwrap_or(0);
        let n = self.inner.get_feature(buf)?;
        self.keep(Op::FeatureGet, Some(id), &buf[..n.min(buf.len())]);
        Ok(n)
    }
    fn report_descriptor(&self, buf: &mut [u8]) -> Result<usize> {
        let n = self.inner.report_descriptor(buf)?;
        self.keep(Op::Descriptor, None, &buf[..n.min(buf.len())]);
        Ok(n)
    }
}

/// Bytes of an answer that tell one unit apart (a serial number), which a driver names so that
/// they are left out of its transcripts.
#[derive(Debug, Clone)]
pub struct Private {
    /// The request, from `at`, after the report number 0 of a device without numbers.
    pub request: &'static [u8],
    pub at: usize,
    /// The bytes of each answer to it (input reports and feature reports, report number first)
    /// written as `xx`.
    pub answer: Range<usize>,
}

/// Writes the bytes `rules` name as `xx` in the answers to their requests.
pub fn anonymize(events: &mut [Event], rules: &[Private]) {
    let mut hide: Option<Range<usize>> = None;
    for event in events.iter_mut() {
        match event.op {
            Op::Write | Op::FeatureSet => {
                let Ok(bytes) = event.bytes() else { continue };
                let payload = if bytes.first() == Some(&0) { &bytes[1..] } else { &bytes[..] };
                hide = rules
                    .iter()
                    .find(|r| payload.get(r.at..).is_some_and(|p| p.starts_with(r.request)))
                    .map(|r| r.answer.clone());
            }
            Op::Read | Op::FeatureGet => {
                let Some(range) = &hide else { continue };
                let Ok(pattern) = event.pattern() else { continue };
                let words: Vec<String> = pattern
                    .iter()
                    .enumerate()
                    .map(|(i, b)| match b {
                        Some(b) if !range.contains(&i) => format!("{b:02x}"),
                        _ => "xx".to_string(),
                    })
                    .collect();
                event.data = words.join(" ");
                event.len = None;
            }
            _ => {}
        }
    }
}
