// src/io/stdstreams.rs -- I/O to using standard input/output handles
// Copyright 2016-2017 the Tectonic Project
// Licensed under the MIT License.

use std::ffi::OsStr;
use std::io::{stdin, stdout, Cursor, Read, Seek, SeekFrom};
use std::rc::Rc;

use errors::Result;
use status::StatusBackend;
use super::{InputFeatures, InputHandle, InputOrigin, IoProvider, OpenResult, OutputHandle};


/// GenuineStdoutIo provides a mechanism for the "stdout" output to actually
/// go to the process's stdout.
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct GenuineStdoutIo {}


impl GenuineStdoutIo {
    pub fn new() -> GenuineStdoutIo {
        GenuineStdoutIo {}
    }
}


impl IoProvider for GenuineStdoutIo {
    fn output_open_stdout(&mut self) -> OpenResult<OutputHandle> {
        // NOTE: keep in sync with io::memory::MemoryIo::stdout_key()
        OpenResult::Ok(OutputHandle::new(OsStr::new(""), stdout()))
    }
}


/// This helper type is needed to get full InputFeatures functionality on a
/// shared, ref-counted Vec<u8>: we're not allowed to implement AsRef<[u8]> on
/// Rc<Vec<u8>> since none of the types or traits come from the Tectonic
/// crate.
#[derive(Clone,Debug,Eq,PartialEq)]
struct SharedByteBuffer(Rc<Vec<u8>>);

impl SharedByteBuffer {
    fn new(data: Vec<u8>) -> SharedByteBuffer {
        SharedByteBuffer(Rc::new(data))
    }
}

impl AsRef<[u8]> for SharedByteBuffer {
    fn as_ref(&self) -> &[u8] {
        &*self.0
    }
}

impl InputFeatures for Cursor<SharedByteBuffer> {
    fn get_size(&mut self) -> Result<usize> {
        Ok(self.get_ref().0.len())
    }

    fn try_seek(&mut self, pos: SeekFrom) -> Result<u64> {
        Ok(self.seek(pos)?)
    }
}


/// BufferedPrimaryIo provides a mechanism for the TeX "primary input"
/// to come from stdin. Because Tectonic makes multiple passes through the
/// input by default, we have to buffer it in memory so that the input can be
/// read multiple times. It wouldn't be hard to make an alternative
/// implementation that skips the buffering and errors out if one tries to
/// open the stream more than once.
///
/// TODO: it might be better to stream stdin to a temporary file on disk that
/// we then delete while holding on to the file handle. But mkstemp-rs doesn't
/// give us Files and the whole approach might get a bit hairy, so we don't do
/// that.
///
/// TODO: it also would be nicer to actually stream through stdin at pace on
/// the first pass rather than slurping it all into memory upon construction,
/// but once more we're being lazy.
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct BufferedPrimaryIo {
    buffer: SharedByteBuffer,
}

impl BufferedPrimaryIo {
    pub fn from_stream<T: Read>(stream: &mut T) -> Result<Self> {
        let mut buf = [0u8; 8192];
        let mut alldata = Vec::<u8>::new();

        loop {
            let nbytes = stream.read(&mut buf)?;

            if nbytes == 0 {
                break;
            }

            alldata.extend_from_slice(&buf[..nbytes]);
        }

        Ok(BufferedPrimaryIo {
            buffer: SharedByteBuffer::new(alldata),
        })
    }

    pub fn from_stdin() -> Result<Self> {
        Self::from_stream(&mut stdin())
    }

    pub fn from_text<T: AsRef<str>>(text: T) -> Self {
        BufferedPrimaryIo {
            buffer: SharedByteBuffer::new(text.as_ref().as_bytes().to_owned())
        }
    }
}


impl IoProvider for BufferedPrimaryIo {
    fn input_open_primary(&mut self, _status: &mut StatusBackend) -> OpenResult<InputHandle> {
        OpenResult::Ok(InputHandle::new(OsStr::new(""), Cursor::new(self.buffer.clone()), InputOrigin::Other))
    }
}
