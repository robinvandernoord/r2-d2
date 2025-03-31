use core::iter::Cycle;
use core::slice::Iter;
use rustic_core::{Progress, ProgressBars};
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex, MutexGuard};
use std::{borrow::Cow, io};

static BAR_LENGTH: usize = 25;

fn human_readable_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes;

    for unit in &units {
        if size < 1024 {
            return format!("{size:.1} {unit}");
        }
        size /= 1024;
    }

    format!("{size:.1} PB") // Fallback for very large sizes
}

#[derive(Clone, Debug)]
pub struct ProgressState {
    prefix: String,
    title: String,
    total: u64,   // value to reach
    current: u64, // current value

    max_char_length: usize, // for the print (to flush afterwards)
    spinner_chars: Cycle<Iter<'static, char>>,
}

impl ProgressState {
    fn new(prefix: impl Into<Cow<'static, str>>) -> Self {
        Self {
            prefix: prefix.into().to_string(),
            ..Default::default()
        }
    }

    fn new_mutex(prefix: impl Into<Cow<'static, str>>) -> Arc<Mutex<Self>> {
        let inner = Self::new(prefix);

        Arc::new(Mutex::new(inner))
    }

    fn title(
        &mut self,
        title: String,
    ) {
        self.title = title;
    }

    fn prefix(&self) -> String {
        let mut result = String::new();

        if !self.prefix.is_empty() {
            result.push_str(&self.prefix);
            result.push(' ');
        }

        if !self.title.is_empty() {
            result.push_str(&self.title);
            result.push(' ');
        }

        result
    }

    fn total(
        &mut self,
        total: u64,
    ) {
        self.total = total;
    }

    fn padding(&self) -> String {
        " ".repeat(self.max_char_length)
    }

    fn inc(
        &mut self,
        length: u64,
    ) -> u64 {
        self.current += length;
        self.current
    }

    #[expect(clippy::cast_precision_loss, reason = "The numbers won't be that big")]
    fn percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.current as f64 / self.total as f64) * 100.0
        }
    }

    fn finish(&mut self) {
        self.current = self.total;

        eprintln!("\r {} ✓{}", self.prefix(), self.padding());
    }

    fn print_with_prefix<S: AsRef<str>, W: Sized + Write>(
        &mut self,
        text: S,
        writer: &mut BufWriter<W>,
    ) -> Option<()> {
        let text = format!("\r{} {}", self.prefix(), text.as_ref());
        self.print(text, writer)
    }

    fn print_with_suffix<S: AsRef<str>, W: Sized + Write>(
        &mut self,
        text: S,
        writer: &mut BufWriter<W>,
    ) -> Option<()> {
        let text = format!("\r{} {}", text.as_ref(), self.prefix());
        self.print(text, writer)
    }

    fn print<S: AsRef<str>, W: Sized + Write>(
        &mut self,
        text: S,
        writer: &mut BufWriter<W>,
    ) -> Option<()> {
        let text_str = text.as_ref();

        if text_str.len() > self.max_char_length {
            self.max_char_length = text_str.len();
        }

        write!(writer, "{text_str}").ok()
    }
}
static SPINNER_CHARS: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            title: String::new(),
            total: 0,
            current: 0,
            max_char_length: 0,
            // for Spinner Type:
            spinner_chars: SPINNER_CHARS.iter().cycle(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ProgressType {
    Hidden,
    Spinner(Arc<Mutex<ProgressState>>),
    Counter(Arc<Mutex<ProgressState>>),
    Bytes(Arc<Mutex<ProgressState>>),
}

impl ProgressType {
    fn get_state(&self) -> Option<MutexGuard<ProgressState>> {
        match self {
            Self::Spinner(s) | Self::Counter(s) | Self::Bytes(s) => {
                // Get a MutexGuard:
                s.lock().ok()
            },
            Self::Hidden => None,
        }
    }
}

impl Progress for ProgressType {
    fn is_hidden(&self) -> bool {
        matches!(self, Self::Hidden)
    }

    fn set_length(
        &self,
        len: u64,
    ) {
        if let Some(mut state) = self.get_state() {
            state.total(len);
        }
    }

    fn set_title(
        &self,
        title: &'static str,
    ) {
        if let Some(mut state) = self.get_state() {
            state.title(title.to_string());

            eprint!("\r {}{}", state.prefix(), state.padding());
        }
    }

    #[expect(
        clippy::cast_sign_loss,
        reason = "This percentage will always be positive"
    )]
    fn inc(
        &self,
        inc: u64,
    ) {
        let mut writer = BufWriter::with_capacity(16, io::stderr());

        let Some(mut state) = self.get_state() else {
            // can't do anything without state
            return;
        };

        let current = state.inc(inc) as usize;
        let percentage = state.percentage();
        let total = state.total;

        let n = percentage.floor() as usize;
        let filled_length = (BAR_LENGTH * n) / 100;

        let _ = match self {
            Self::Hidden => None,
            Self::Spinner(_) => {
                let mut result = None;
                if let Some(frame) = state.spinner_chars.next() {
                    result = state.print_with_suffix(format!(" {frame} "), &mut writer);
                }
                result
            },
            Self::Counter(_) => state.print_with_suffix(
                format!(
                    "[{}{}{}] {}%:",
                    "=".repeat(filled_length),
                    if filled_length < BAR_LENGTH { ">" } else { "" },
                    " ".repeat(BAR_LENGTH.saturating_sub(filled_length + 1)),
                    n
                ),
                &mut writer,
            ),
            Self::Bytes(_) => {
                let readable_current = human_readable_size(current as u64);
                let readable_total = human_readable_size(total);

                state.print_with_suffix(
                    format!(
                        "[{}{}{}] {}% ({}/{}):",
                        "=".repeat(filled_length),
                        if filled_length < BAR_LENGTH { ">" } else { "" },
                        " ".repeat(BAR_LENGTH.saturating_sub(filled_length + 1)),
                        n,
                        readable_current,
                        readable_total
                    ),
                    &mut writer,
                )
            },
        }
        .and_then(|()| writer.flush().ok());
    }

    fn finish(&self) {
        if let Some(mut state) = self.get_state() {
            state.finish();
        }
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Default, Hash)]
pub struct ProgressBar {}

impl ProgressBars for ProgressBar {
    type P = ProgressType;

    fn progress_hidden(&self) -> Self::P {
        ProgressType::Hidden
    }

    fn progress_spinner(
        &self,
        prefix: impl Into<Cow<'static, str>>,
    ) -> Self::P {
        ProgressType::Spinner(ProgressState::new_mutex(prefix))
    }

    fn progress_counter(
        &self,
        prefix: impl Into<Cow<'static, str>>,
    ) -> Self::P {
        ProgressType::Counter(ProgressState::new_mutex(prefix))
    }

    fn progress_bytes(
        &self,
        prefix: impl Into<Cow<'static, str>>,
    ) -> Self::P {
        ProgressType::Bytes(ProgressState::new_mutex(prefix))
    }
}

// pub(crate) fn test_progressbar() {
//     let pb = ProgressBar::default();
//
//     // let spinner = pb.progress_spinner("backing up");
//     // let spinner = pb.progress_counter("backing up");
//     let spinner = pb.progress_bytes("backing up");
//     spinner.set_length(100);
//     spinner.set_title("myfile.txt");
//
//     let n = 100;
//     for _ in 0..n {
//         spinner.inc(1);
//         std::thread::sleep(std::time::Duration::from_millis(100));
//     }
//
//     spinner.finish();
//
//     eprintln!("the end");
// }
