// This all needs cleanup: we eventually want some kind of stackable set of
// layers that we investigated for files to read or write. But for now it gets
// the job done.

use libc;
use mktemp::Temp;
use std::ffi::OsString;
use std::fs::File;
use std::io::{copy, stderr, Read, Seek, Write};
use std::os::unix::io::{IntoRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zip::result::{ZipError, ZipResult};
use zip::ZipArchive;

#[derive(Clone,Copy,Debug)]
pub enum FileFormat {
    TFM,
    Pict,
    Tex,
    Format,
}


pub fn c_format_to_rust (format: libc::c_int) -> Option<FileFormat> {
    // See the kpse_file_format_type enum.
    match format {
        3 => Some(FileFormat::TFM),
        10 => Some(FileFormat::Format),
        25 => Some(FileFormat::Pict),
        26 => Some(FileFormat::Tex),
        _ => None
    }
}

fn format_to_extension (format: FileFormat) -> &'static str {
    match format {
        FileFormat::TFM => ".tfm",
        FileFormat::Pict => ".pdf", /* XXX */
        FileFormat::Tex => ".tex",
        FileFormat::Format => ".fmt",
    }
}


struct FinderState<R: Read + Seek> {
    zip: ZipArchive<R>
}

impl<R: Read + Seek> FinderState<R> {
    pub fn new (reader: R) -> ZipResult<FinderState<R>> {
        ZipArchive::new(reader).map (|zip|
            FinderState {
                zip: zip
            }
        )
    }

    fn zip_to_temp_fd (&mut self, name: &Path) -> Result<RawFd,ZipError> {
        let mut zipitem = match self.zip.by_name (name.to_str ().unwrap ()) {
            Err(e) => return Err(e),
            Ok(f) => f
        };

        let temp_file = Temp::new_file ().unwrap ();
        {
            let mut f = File::create (temp_file.to_path_buf ()).unwrap ();
            copy (&mut zipitem, &mut f).unwrap ();
        }

        let f = File::open (temp_file.to_path_buf ()).unwrap ();
        Ok(f.into_raw_fd ())
    }

    pub fn get_readable_fd<'a> (&'a mut self, name: &'a Path, format: FileFormat, _: bool) -> Option<RawFd> {
        /* See if a file's in the bundle. If so, we need to extract the
         * contents to a temporary file that we then unlink, because: (1) the
         * format file is read in as a gzip file, and the way that it is
         * created requires that the file be associated with a Unix file
         * handle. But (2) the file must be seekable, so we can't just use
         * pipes. The temp file is unlinked at the end of this function, but
         * the open file handle keeps it around for as long as the progam
         * needs it. Yay Unix!
         *
         * We need to use the zip_to_temp_fd helper because the first ZipResult
         * we look at keeps a mutable borrow on the ZipArchive.
         */

        let mut ext = PathBuf::from (name); // XXX redundant code
        let mut ename = OsString::from (ext.file_name ().unwrap ());
        ename.push (format_to_extension (format));
        ext.set_file_name (ename);

        if let Ok(fd) = self.zip_to_temp_fd (name) {
            return Some(fd);
        }

        return match self.zip_to_temp_fd (&ext) {
            Err(e) => {
                if let ZipError::FileNotFound = e {
                    writeln!(&mut stderr(), "PKGW: failed to locate: {:?}", name).expect ("stderr failed");
                    None
                } else {
                    panic!("error reading bundle: {}", e)
                }
            },
            Ok(fd) => Some(fd)
        };
    }
}


// Finding files through the global singleton FinderState instance.

lazy_static! {
    static ref SINGLETON: Mutex<Option<FinderState<File>>> = {
        Mutex::new(None)
    };
}

pub fn open_bundle (path: &Path) -> () {
    let file = File::open(path).unwrap ();
    let mut s = SINGLETON.lock().unwrap();
    *s = Some(FinderState::new (file).unwrap ());
}

pub fn get_readable_fd (name: &Path, format: FileFormat, must_exist: bool) -> Option<RawFd> {
    /* We currently don't care about must_exist. */

    let mut s = SINGLETON.lock ().unwrap ();

    /* For now: if we can open straight off of the filesystem, do that. No
     * bundle needed. */

    if let Ok(f) = File::open (name) {
        return Some(f.into_raw_fd());
    }

    let mut ext = PathBuf::from (name);
    let mut ename = OsString::from (ext.file_name ().unwrap ());
    ename.push (format_to_extension (format));
    ext.set_file_name (ename);

    if let Ok(f) = File::open (ext.clone ()) {
        return Some(f.into_raw_fd());
    }

    /* If the global bundle has been opened, see if it's got the file. */

    match *s {
        Some(ref mut fstate) => fstate.get_readable_fd(name, format, must_exist),
        None => None
    }
}
