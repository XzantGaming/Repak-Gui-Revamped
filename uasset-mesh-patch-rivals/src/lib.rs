use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::cmp::max;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::io;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

static mut BULK_OFFSET: usize = 0;
const UASSET_MAGIC: usize = 0x9E2A83C1;
static mut FINAL_SIZE_OFFSET: u64 = 0;
static mut MAT_COUNT: i32 = 0;

pub trait Logger{
    fn log<S: Into<String>>(&self,buf: S);
}
pub struct PatchFixer<LogProvider: Logger>{
    pub logger: LogProvider
}

macro_rules! log {
    ($logger:expr, $($arg:tt)*) => {
        $logger.log(format!($($arg)*));
    };
}


impl<LogProvider: Logger> PatchFixer<LogProvider> {
    pub fn read_uasset<R: Read + Seek>(&self, f: &mut R) -> Result<(u32, u32), io::Error> {
        let magic = f.read_u32::<LittleEndian>()?;
        if magic != UASSET_MAGIC as u32 {
            log!(&self.logger,"UASSET not found");          
            panic!();
        }
    
        let legacyfileversion = f.read_i32::<LittleEndian>()?;
        if legacyfileversion != -4 {
            f.seek_relative(4)?;
        }
    
        let mut _fileversion_ue5 = 0;
        let fileversion_ue4 = f.read_i32::<LittleEndian>()?;
        if fileversion_ue4 != 0 {
            log!(self.logger,"Unsupported UE4 file version");
            panic!();
        }
    
        if legacyfileversion <= -8 {
            _fileversion_ue5 = f.read_i32::<LittleEndian>()?;
        }
    
        let _file_version_licensee_ue = f.read_i32::<LittleEndian>()?;
    
        if legacyfileversion <= -2 {
            let _num_custom_versions = f.read_i32::<LittleEndian>()?;
            for _ in 0.._num_custom_versions {
                f.seek_relative(16)?; // Skip ID
                f.seek_relative(4)?; // Skip verNum
            }
        }
    
        let _section_six_offset = f.read_u32::<LittleEndian>()?;
        let _folder_name_len = f.read_u32::<LittleEndian>()?;
        let mut _folder_name_buf = vec![0; _folder_name_len as usize];
        f.read_exact(&mut _folder_name_buf)?;
        let _folder_name = String::from_utf8(_folder_name_buf)
            .unwrap()
            .trim_end_matches('\0')
            .to_string();
        let _package_flags = f.read_u32::<LittleEndian>()?;
        let _name_count = f.read_u32::<LittleEndian>()?;
        let _name_offset = f.read_u32::<LittleEndian>()?;
    
        // Assuming here that objectverUE5 is data_esource
        let _soft_object_path_count = f.read_u32::<LittleEndian>()?;
        let _soft_object_path_offset = f.read_u32::<LittleEndian>()?;
    
        // Assuming here that objectver is automatic ve
        let _gatherable_text_data_count = f.read_u32::<LittleEndian>()?;
        let _gatherable_text_data_offset = f.read_u32::<LittleEndian>()?;
    
        let export_count = f.read_u32::<LittleEndian>()?;
        let export_offset = f.read_u32::<LittleEndian>()?;
        let _import_count = f.read_u32::<LittleEndian>()?;
        let _import_offset = f.read_u32::<LittleEndian>()?;
        let _depends_offset = f.read_u32::<LittleEndian>()?;
        let _soft_package_references_count = f.read_u32::<LittleEndian>()?;
        let _soft_package_references_offset = f.read_u32::<LittleEndian>()?;
        let _searchable_names_offset = f.read_u32::<LittleEndian>()?;
        let _thumbnail_table_offset = f.read_u32::<LittleEndian>()?;
    
        f.seek_relative(16)?; // Skip GUID
    
        let generation_count = f.read_u32::<LittleEndian>()?;
        for _ in 0..generation_count {
            f.seek_relative(4)?; // Skip
            f.seek_relative(4)?; // Skip
        }
    
        f.seek_relative(10)?; // Skip
    
        let name_len = f.read_u32::<LittleEndian>()?;
        let mut name_buf = vec![0; name_len as usize];
        f.read_exact(&mut name_buf)?;
        let _name = String::from_utf8(name_buf)
            .unwrap()
            .trim_end_matches('\0')
            .to_string();
    
        f.seek_relative(10)?; // Skip
    
        let name_len = f.read_u32::<LittleEndian>()?;
        let mut name_buf = vec![0; name_len as usize];
        f.read_exact(&mut name_buf)?;
        let _name = String::from_utf8(name_buf)
            .unwrap()
            .trim_end_matches('\0')
            .to_string();
    
        f.seek_relative(4)?; // Skip
        f.seek_relative(4)?; // Skip
        f.seek_relative(4)?; // Skip
    
        let _num_additional_cookie_packages = f.read_u32::<LittleEndian>()?;
        for _ in 0.._num_additional_cookie_packages {
            let name_len = f.read_u32::<LittleEndian>()?;
            let mut name_buf = vec![0; name_len as usize];
            f.read_exact(&mut name_buf)?;
            let _name = String::from_utf8(name_buf)
                .unwrap()
                .trim_end_matches('\0')
                .to_string();
        }
    
        let _asset_regen_data_offset = f.read_u32::<LittleEndian>()?;
    
        unsafe {
            BULK_OFFSET = f.stream_position()? as usize;
        }
        let _bulk_data_start_offset = f.read_i64::<LittleEndian>()?;
        Ok((export_count, export_offset))
    }
    
    pub fn read_exports<R: Read + Seek>(
        &mut self,
        f: &mut R,
        size_buf: &mut Vec<i64>,
        offset_buf: &mut Vec<i64>,
        exp_offset: u32,
        exp_cnt: u32,
    ) -> Result<(), io::Error> {
        let export_offset = exp_offset;
        let export_count = exp_cnt;
        f.seek(SeekFrom::Start(export_offset as u64))?;
    
        for i in 0..export_count {
            let _class_index = f.read_i32::<LittleEndian>()?;
            let _super_index = f.read_i32::<LittleEndian>()?;
            let _template_index = f.read_u32::<LittleEndian>()?;
            let _outer_index = f.read_u32::<LittleEndian>()?;
            let _name_map_pointer = f.read_u32::<LittleEndian>()?;
            let _number = f.read_u32::<LittleEndian>()?;
            let _object_flags = f.read_u32::<LittleEndian>()?;
    
            // Assuming Asset Object Version is Automatic
            #[allow(static_mut_refs)]
            if i == export_count - 1 {
                unsafe {
                    FINAL_SIZE_OFFSET = f.stream_position()?;
                    log!(self.logger,"\tFinalSizeOffset: {:#02X}", FINAL_SIZE_OFFSET);
                }
            }
    
            let serial_size = f.read_i64::<LittleEndian>()?;
            let serial_offset = f.read_i64::<LittleEndian>()?;
    
            size_buf.push(serial_size);
            offset_buf.push(serial_offset);
    
            let _forced_export = f.read_i32::<LittleEndian>()?;
            let _not_for_client = f.read_i32::<LittleEndian>()?;
            let _not_for_server = f.read_i32::<LittleEndian>()?;
            let _inherited_instance = f.read_i32::<LittleEndian>()?;
            let _package_flags = f.read_u32::<LittleEndian>()?;
            let _always_loaded_for_editor_game = f.read_i32::<LittleEndian>()?;
            let _is_asset = f.read_i32::<LittleEndian>()?;
            let _generate_public_hash = f.read_i32::<LittleEndian>()?;
            let _first_export_dependency = f.read_u32::<LittleEndian>()?;
            let _serialization_before_serialization_dependencies_size = f.read_u32::<LittleEndian>()?;
            let _create_before_serialization_dependencies_size = f.read_u32::<LittleEndian>()?;
            let _serialization_before_create_dependencies_size = f.read_u32::<LittleEndian>()?;
            let _create_before_create_dependencies_size = f.read_u32::<LittleEndian>()?;
        }
    
        Ok(())
    }
    
    pub fn read_uexp<R: Read + Seek, W: Write + Seek>(
        &self,
        r: &mut R,
        file_size: u64,
        file_name: &str,
        o: &mut W,
        offsets: &Vec<i64>,
    ) -> io::Result<()> {
        let mut material_count = 0;
        let final_offset = offsets.last().unwrap() - file_size as i64;
    
        // Assuming last export is the one we want
        let mut buffer = vec![0; final_offset as usize];
        r.read_exact(&mut buffer)?;
        o.write_all(&buffer)?;
    
        log!(self.logger,            "\tStarting search for data at Offset: {:#01X}",
            r.stream_position()?
        );
    
        // Dirty way of finding what we need
        let max_mat_count = 255;
        let mut max_bytes: i32 = 500000;
        let mut current_bytes = 0;
        let starting_pos = r.stream_position()?;
    
        let mut found = false;
        'primary: loop {
            if current_bytes > max_bytes {
                log!(self.logger,
                    "\tFailed to find data within range {:#1X} - {:#1X}",
                    starting_pos,
                    r.stream_position()?
                );
    
                if max_bytes == file_size as i32 {
                    break;
                }
                max_bytes = max(max_bytes + 0x50000, (file_size - starting_pos) as i32);
                log!(self.logger,"\tIncreasing range to {}", max_bytes);
    
                continue;
            }
    
            let mut checked_bytes = [0; 3];
            match r.read_exact(&mut checked_bytes) {
                Ok(_) => (),
                Err(_) => {
                    break;
                }
            }
            current_bytes += 3;
            if checked_bytes == [0xff, 0xff, 0xff] {
                let x = r.read_u8()?;
                if x != 0xff {
                    r.seek_relative(-1)?;
                } else {
                    current_bytes += 1;
                    continue 'primary;
                }
    
                r.seek_relative(-8)?;
                material_count = r.read_i32::<LittleEndian>()?;
    
                if material_count > 0 && material_count < max_mat_count {
                    found = true;
                    break;
                } else {
                    r.seek_relative(4)?;
                }
            } else {
                current_bytes -= 2;
                r.seek_relative(-2)?;
            }
        }
        
        if !found {
            print!("{}", "\tNo suitable materials found... ");
            if file_name.contains("Skeleton")
                || file_name.contains("Physics")
                || file_name.contains("Anim")
                || file_name.contains("Rig")
                || file_name.contains("BP")
                || !file_name.contains("SK_")
            // if we fail to patch and it isnt a SK_ mod, we can probably skip it without issues
            {
                log!(self.logger,"{}", "but we can try skipping this!");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Skip this"));
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid mesh provided".to_string(),
            ));
        }
        log!(self.logger,"{} {:1X}", "\t\tFound data at Offset: ", r.stream_position()?);
        let ending_pos = r.stream_position()?;
    
        r.seek(SeekFrom::Start(starting_pos))?;
        let mut buffer = vec![0; (ending_pos - starting_pos) as usize];
        r.read_exact(&mut buffer)?;
        o.write_all(&buffer)?;
    
        log!(self.logger,"\tFound {} materials", material_count.to_string());
    
        for _ in 0..material_count {
            let mut buffer = vec![0; 40];
            r.read_exact(&mut buffer)?;
            o.write_all(&buffer)?;
            o.write_all(&[0x0, 0x0, 0x0, 0x0])?;
        }
    
        let mut buffer = Vec::new();
        r.read_to_end(&mut buffer)?;
        o.write_all(&buffer)?;
    
        o.flush()?;
        unsafe {
            MAT_COUNT = material_count;
        }
    
        Ok(())
    }
    
    pub unsafe fn clean_uasset(&mut self,file: PathBuf, sizes: &[i64]) -> io::Result<()> {
        log!(self.logger,"\tStarting Asset Cleaning...");
    
        let final_size = sizes.last().unwrap() + (4 * MAT_COUNT as i64);
    
        let mut f = OpenOptions::new().read(true).write(true).open(file)?;
    
        f.seek(SeekFrom::Start(FINAL_SIZE_OFFSET))?;
        f.write_i64::<LittleEndian>(final_size)?;
        f.flush()?;
    
        f.seek(SeekFrom::Start(BULK_OFFSET as u64))?;
        let bulk_start_offset = f.read_i64::<LittleEndian>()?;
        f.seek(SeekFrom::Current(-8))?;
        let fixed_offset = bulk_start_offset + (4 * MAT_COUNT as i64);
        f.write_i64::<LittleEndian>(fixed_offset)?;
        f.flush()?;
    
        println!("{}", "\tAsset Cleaning Complete!");
        Ok(())
    }
}

/// Public API for processing mesh files
pub fn process_mesh_file(uasset_path: &Path, uexp_path: &Path) -> Result<(), io::Error> {
    let mut logger = PrintLogger;
    let mut fixer = PatchFixer { logger };
    
    // Read the uasset file
    let mut uasset_file = BufReader::new(File::open(uasset_path)?);
    let (export_count, export_offset) = fixer.read_uasset(&mut uasset_file)?;
    
    // Read exports
    let mut size_buf = Vec::new();
    let mut offset_buf = Vec::new();
    fixer.read_exports(&mut uasset_file, &mut size_buf, &mut offset_buf, export_offset, export_count)?;
    
    // Process the uexp file
    let mut uexp_file = BufReader::new(File::open(uexp_path)?);
    let uexp_size = std::fs::metadata(uexp_path)?.len();
    
    // Create temporary output file
    let temp_path = uexp_path.with_extension("uexp.tmp");
    let mut output_file = BufWriter::new(File::create(&temp_path)?);
    
    let file_name = uasset_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    
    // Process the mesh data
    fixer.read_uexp(&mut uexp_file, uexp_size, file_name, &mut output_file, &offset_buf)?;
    
    // Clean up the uasset file
    unsafe {
        fixer.clean_uasset(uasset_path.to_path_buf(), &size_buf)?;
    }
    
    // Replace the original uexp file
    std::fs::rename(temp_path, uexp_path)?;
    
    Ok(())
}

/// Check if a file is a mesh uasset based on content analysis
pub fn is_mesh_uasset(path: &Path) -> Result<bool, io::Error> {
    let mut file = File::open(path)?;
    
    // Check magic number
    let magic = file.read_u32::<LittleEndian>()?;
    if magic != UASSET_MAGIC as u32 {
        return Ok(false);
    }
    
    // Basic heuristics for mesh detection
    let file_name = path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let is_mesh = file_name.contains("sk_") || 
                  file_name.contains("sm_") ||
                  file_name.contains("mesh") ||
                  file_name.contains("skeletal") ||
                  file_name.contains("static");
    
    Ok(is_mesh)
}

pub struct PrintLogger;

impl Logger for PrintLogger {
    fn log<S: Into<String>>(&self, buf: S) {
        println!("{}", buf.into());
    }
}
