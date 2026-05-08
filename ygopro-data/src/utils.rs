pub mod string {
    #![allow(dead_code)]

    use std::ops::Deref;
    use once_cell::sync::OnceCell;

    use binrw::BinRead;
    use binrw::BinWrite;
    use binrw::binrw;
    use binrw::helpers::until_eof;

    /// transform \[u16\] to string. \
    /// return [`None`] if it's illegal.
    pub fn cast_to_string(array: &[u16]) -> Option<String> {
        let mut str = array;
        if let Some(index) = array.iter().position(|&i| i == 0) {
            str = &str[0..index as usize];
        }
        else { return None }
        let body = unsafe { std::slice::from_raw_parts(str.as_ptr() as *const u8, str.len() * 2) };
        let (cow, _, had_errors) = encoding_rs::UTF_16LE.decode(&body);
        if had_errors { None }
        else { Some(cow.to_string()) }
    }

    /// Transform string to \[u16\] without length limit but a \0 in the end.
    pub fn cast_to_c_array(message: &str) -> Vec<u16> {
        let mut vector: Vec<u16> = message.encode_utf16().collect();
        vector.push(0);
        vector
    }

    /// Transform string to \[u16\] with a fixed size. \
    /// Differennt from ygopro, it will keeps 0 for residual part.
    pub fn cast_to_fix_length_array<const N: usize>(message: &str) -> [u16; N] {
        let mut data = [0u16; N];
        for (index, chr) in message.encode_utf16().enumerate() {
            data[index] = chr;
        }
        data
    }

    #[derive(Clone, BinRead, BinWrite)]
    pub struct FixedLengthString<const L: usize> {
        data: [u16; L],
        #[brw(ignore)]
        str: OnceCell<String>
    }

    impl<const L: usize> std::fmt::Display for FixedLengthString<L> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", &cast_to_string(&self.data).unwrap_or("[ERROR]".to_string()))
        }
    }

    impl <const L: usize> std::fmt::Debug for FixedLengthString<L> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "FixedLengthString[{:}] ", L)?;
            write!(f, "\"{}\"", &cast_to_string(&self.data).unwrap_or("[ERROR]".to_string()))   
        }
    }

    impl<const L: usize> FixedLengthString<L> {
        pub fn new(str: String) -> Self {
            Self {
                data: cast_to_fix_length_array(&str),
                str: OnceCell::with_value(str)
            }
        }

        pub fn resolve_data(&mut self) {
            if self.str.get() == None {
                if let Some(str) = cast_to_string(&self.data) {
                    self.str.set(str).ok();
                }
            }
        }

        pub fn resolve_str(&mut self) {
            if let Some(str) = self.str.get() {
                self.data = cast_to_fix_length_array(str);
            }
        }        
    }

    impl<const L: usize> Deref for FixedLengthString<L> {
        type Target = str;

        fn deref(&self) -> &Self::Target {
            self.str
                .get_or_try_init(|| cast_to_string(&self.data).ok_or(""))
                .map(|s| s.as_str())
                .unwrap_or("")
        }
    }

    impl<const L: usize> From<String> for FixedLengthString<L> {
        fn from(value: String) -> Self {
            FixedLengthString::new(value)
        }
    }

    impl<'s, const L: usize> From<&'s str> for FixedLengthString<L> {
        fn from(value: &'s str) -> Self {
            FixedLengthString::new(value.to_string())
        }
    }

    #[binrw]
    pub struct U16String {
        #[br(parse_with=until_eof)]
        data: Vec<u16>,
        #[brw(ignore)]
        str: OnceCell<String>,
    }

    impl std::fmt::Debug for U16String {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "U16String[{:}] ", self.data.len())?;
            write!(f, "\"{:}\"", &cast_to_string(&self.data).unwrap_or("[ERROR]".to_string()))
        }
    }
    
    impl std::fmt::Display for U16String {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "U16String[{:}] ", self.data.len())?;
            write!(f, "\"{:}\"", &cast_to_string(&self.data).unwrap_or("[ERROR]".to_string()))
        }
    }

    impl U16String {
        pub fn new(str: String) -> Self {
            Self {
                data: cast_to_c_array(&str),
                str: OnceCell::with_value(str)
            }
        }

        pub fn resolve_data(&self) {
            if self.str.get() == None {
                if let Some(str) = cast_to_string(&self.data) {
                    self.str.set(str).ok();
                }
            }
        }

        pub fn resolve_str(&mut self) {
            if let Some(str) = self.str.get() {
                self.data = cast_to_c_array(str);
            }
        }        
    }

    impl Deref for U16String {
        type Target = str;

        fn deref(&self) -> &Self::Target {
            self.str
                .get_or_try_init(|| cast_to_string(&self.data).ok_or(""))
                .map(|s| s.as_str())
                .unwrap_or("")
        }
    }

    impl From<String> for U16String {
        fn from(value: String) -> Self {
            U16String::new(value)
        }
    }

    impl<'s> From<&'s str> for U16String {
        fn from(value: &'s str) -> Self {
            U16String::new(value.to_string())
        }
    }

    impl<'s> From<&'s [u16]> for U16String {
        fn from(value: &'s [u16]) -> Self {
            U16String {
                data: value.to_vec(),
                str: OnceCell::new()
            }
        }
    }
}

