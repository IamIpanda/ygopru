// ygopro lib: server logic, no network binding
// 对应: ../ygopro/gframe/ (server branch, YGOPRO_SERVER_MODE)

mod constants;
mod single_duel;
mod tag_duel;

use std::collections::HashMap;
use std::ffi::c_char;
use std::ffi::CStr;
use std::sync::OnceLock;

use ygopro_data::message::ctos::MessageType as CtosMsg;
use ygopro_data::message::stoc::MessageType as StocMsg;
use ygopro_data::data::LFList;

use crate::constants::*;

// pub use crate::constants::MAX_MATCH_COUNT;

// // 对应: ../ygopro/gframe/data_manager.cpp:13 (global dataManager)
// static GLOBAL_DATA_MANAGER: OnceLock<DataManager> = OnceLock::new();

// pub fn set_global_data_manager(dm: DataManager) {
//     let _ = GLOBAL_DATA_MANAGER.set(dm);
// }

// // 对应: ../ygopro/gframe/data_manager.cpp:507 (CardReader)
// extern "C" fn card_reader_callback(code: u32, data: *mut ygopro_core_wrapper::CoreCard) -> u32 {
//     if let Some(dm) = GLOBAL_DATA_MANAGER.get() {
//         let cd = match dm.datas.get(&code) { Some(c) => c, None => return 0 };
//         unsafe {
//             (*data).code = cd.code;
//             (*data).alias = cd.alias;
//             (*data).setcode.copy_from_slice(&cd.setcode);
//             (*data).card_type = cd.card_type;
//             (*data).level = cd.level;
//             (*data).attribute = cd.attribute;
//             (*data).race = cd.race;
//             (*data).attack = cd.attack;
//             (*data).defense = cd.defense;
//             (*data).lscale = cd.lscale;
//             (*data).rscale = cd.rscale;
//             (*data).link_marker = cd.link_marker;
//             (*data).rule_code = cd.rule_code;
//             if cd.card_type & (0x40 | 0x2000 | 0x800000 | 0x4000000) != 0 {
//                 (*data).defense = cd.link_marker as i32;
//                 (*data).link_marker = 0;
//             } else {
//                 (*data).link_marker = 0;
//             }
//         }
//         1
//     } else { 0 }
// }

// // 对应: ../ygopro/gframe/data_manager.cpp:512-550 (ScriptReaderEx)
// extern "C" fn script_reader_callback(script_name: *const c_char, slen: *mut i32) -> *mut u8 {
//     use std::path::Path;
//     if script_name.is_null() || slen.is_null() { return std::ptr::null_mut(); }
//     let name = unsafe { CStr::from_ptr(script_name).to_string_lossy() };
//     let path = Path::new(name.as_ref());
//     if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
//         let special_path = format!("./specials/{}", fname);
//         if let Ok(data) = std::fs::read(&special_path) { unsafe { *slen = data.len() as i32; } return data.leak().as_mut_ptr(); }
//     }
//     if let Ok(data) = std::fs::read(name.as_ref()) { unsafe { *slen = data.len() as i32; } return data.leak().as_mut_ptr(); }
//     if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
//         for dir in &["expansions", "script"] {
//             let p = format!("./{}/{}", dir, fname);
//             if let Ok(data) = std::fs::read(&p) { unsafe { *slen = data.len() as i32; } return data.leak().as_mut_ptr(); }
//         }
//     }
//     std::ptr::null_mut()
// }

// // 对应: ../ygopro/gframe/network.h:199-205 (DuelPlayer)
// #[derive(Debug, Clone)]
// pub struct Client {
//     pub name: [u16; 20],
//     pub client_type: u8,
//     pub state: u8,
// }
// impl Default for Client {
//     fn default() -> Self { Self { name: [0u16; 20], client_type: 0xff, state: 0xff } }
// }

// #[derive(Debug, Clone)]
// pub enum OutgoingTarget { Single(usize), Multi(Vec<usize>) }

// #[derive(Debug, Clone)]
// pub struct OutgoingMessage { pub targets: OutgoingTarget, pub data: Vec<u8> }

// // 对应: ../ygopro/gframe/network.h:21-34 (HostInfo)
// #[derive(Debug, Clone, Copy)]
// pub struct HostInfo {
//     pub lflist: u32, pub rule: u8, pub mode: u8, pub duel_rule: u8,
//     pub no_check_deck: bool, pub no_shuffle_deck: bool,
//     pub start_lp: i32, pub start_hand: u8, pub draw_count: u8, pub time_limit: u16,
// }
// impl Default for HostInfo {
//     fn default() -> Self {
//         Self { lflist: 0, rule: 0, mode: 0, duel_rule: ygopro_core_wrapper::CURRENT_RULE,
//             no_check_deck: false, no_shuffle_deck: false,
//             start_lp: 8000, start_hand: 5, draw_count: 1, time_limit: 180 }
//     }
// }

// // 对应: ../ygopro/gframe/network.h:213-248 (DuelMode)
// pub trait DuelMode: Send {
//     fn host_info(&self) -> HostInfo;
//     fn host_info_mut(&mut self) -> &mut HostInfo;
//     fn pduel(&self) -> Option<ygopro_core_wrapper::intptr_t>;
//     fn duel_stage(&self) -> u8;
//     fn join_game(&mut self, client_id: usize, data: &[u8], is_creator: bool, srv: &mut YgoproServer);
//     fn leave_game(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn to_duelist(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn to_observer(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn player_ready(&mut self, client_id: usize, is_ready: bool, srv: &mut YgoproServer);
//     fn player_kick(&mut self, client_id: usize, pos: u8, srv: &mut YgoproServer);
//     fn update_deck(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer);
//     fn start_duel(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn hand_result(&mut self, client_id: usize, res: u8, srv: &mut YgoproServer);
//     fn tp_result(&mut self, client_id: usize, tp: u8, srv: &mut YgoproServer);
//     fn chat(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer);
//     fn get_response(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer);
//     fn time_confirm(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn surrender(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn request_field(&mut self, client_id: usize, srv: &mut YgoproServer);
//     fn process(&mut self, srv: &mut YgoproServer);
//     fn end_duel(&mut self, srv: &mut YgoproServer);
//     fn tick(&mut self) -> Vec<OutgoingMessage>;
//     fn analyze(&mut self, msgbuffer: &[u8], srv: &mut YgoproServer) -> i32;
// }

// // 对应: ../ygopro/gframe/game.h:167-669 (Game class, server parts)
// pub struct YgoproServer {
//     pub data_manager: DataManager,
//     pub lflists: Vec<LFList>,
//     pub clients: HashMap<usize, Client>,
//     next_client_id: usize,
//     pub server_port: u16, pub replay_mode: u8,
//     pub game_info: HostInfo,
//     pub pre_seed: [[u32; ygopro_core_wrapper::SEED_COUNT]; MAX_MATCH_COUNT],
//     pub pre_seed_specified: [u8; MAX_MATCH_COUNT],
//     pub duel_mode: Option<Box<dyn DuelMode + Send>>,
//     pub pending_messages: Vec<OutgoingMessage>,
// }

// impl YgoproServer {
//     // 对应: ../ygopro/gframe/game.cpp:99-101 (MainServerLoop init)
//     pub fn new() -> Self {
//         Self { data_manager: DataManager::new(), lflists: Vec::new(), clients: HashMap::new(),
//             next_client_id: 1, server_port: 7911, replay_mode: 0, game_info: HostInfo::default(),
//             pre_seed: [[0u32; ygopro_core_wrapper::SEED_COUNT]; MAX_MATCH_COUNT],
//             pre_seed_specified: [0u8; MAX_MATCH_COUNT], duel_mode: None, pending_messages: Vec::new() }
//     }

//     // 对应: ../ygopro/gframe/game.cpp:99-101 (LoadDB + LoadLFList)
//     pub fn initialize(&mut self) -> Result<(), String> {
//         self.data_manager.load_db("cards.cdb").map_err(|e| format!("Failed to load cards.cdb: {}", e))?;
//         self.lflists = ygopro_data::data::load_lflist_all().map_err(|e| format!("Failed to load lflist: {}", e))?;
//         Ok(())
//     }

//     pub fn add_client(&mut self) -> usize { let id = self.next_client_id; self.next_client_id += 1; self.clients.insert(id, Client::default()); id }
//     pub fn remove_client(&mut self, client_id: usize) { self.clients.remove(&client_id); }
//     pub fn get_client(&self, client_id: usize) -> Option<&Client> { self.clients.get(&client_id) }
//     pub fn get_client_mut(&mut self, client_id: usize) -> Option<&mut Client> { self.clients.get_mut(&client_id) }

//     pub fn send_to(&mut self, target: usize, data: Vec<u8>) { self.pending_messages.push(OutgoingMessage { targets: OutgoingTarget::Single(target), data }); }
//     pub fn send_to_multi(&mut self, targets: Vec<usize>, data: Vec<u8>) { self.pending_messages.push(OutgoingMessage { targets: OutgoingTarget::Multi(targets), data }); }

//     // 对应: ../ygopro/gframe/netserver.cpp:23-46 (InitDuel)
//     pub fn init_duel(&mut self) {
//         let mode = self.game_info.mode;
//         self.duel_mode = match mode { 0 => Some(Box::new(single_duel::SingleDuel::new(false)) as Box<dyn DuelMode + Send>), 1 => Some(Box::new(single_duel::SingleDuel::new(true)) as Box<dyn DuelMode + Send>), 2 => Some(Box::new(tag_duel::TagDuel::new()) as Box<dyn DuelMode + Send>), _ => None };
//         if let Some(ref mut dm) = self.duel_mode {
//             *dm.host_info_mut() = self.game_info;
//             let idx = self.game_info.lflist as i32;
//             dm.host_info_mut().lflist = if idx < 0 || idx as usize >= self.lflists.len() { 0 } else { self.lflists[idx as usize].hash };
//         }
//     }

//     // 对应: ../ygopro/gframe/netserver.cpp:264-475 (HandleCTOSPacket)
//     pub fn process_client_data(&mut self, client_id: usize, data: &[u8]) {
//         if data.len() < 1 { return; }
//         let msg = CtosMsg::from(data[0]);
//         let payload = &data[1..];
//         let client_state = self.clients.get(&client_id).map(|c| c.state).unwrap_or(0xff);
//         if msg != CtosMsg::Surrender && msg != CtosMsg::Chat && msg != CtosMsg::RequestField
//             && (client_state == 0xff || (client_state != 0 && client_state != u8::from(msg))) { return; }

//         let mut dm = self.duel_mode.take();
//         match msg {
//             CtosMsg::PlayerInfo => { if payload.len() >= 40 { if let Some(c) = self.clients.get_mut(&client_id) { c.name[..20].copy_from_slice(unsafe { std::slice::from_raw_parts(payload.as_ptr() as *const u16, 20) }); } } }
//             CtosMsg::CreateGame => {
//                 if dm.is_some() { self.duel_mode = dm; return; }
//                 if payload.len() < 100 { self.duel_mode = dm; return; }
//                 let mut info = HostInfo::default(); let mut ofs = 0;
//                 info.lflist = read_buffer_u32(payload, &mut ofs);
//                 info.rule = payload[ofs]; ofs += 1; info.mode = payload[ofs]; ofs += 1; info.duel_rule = payload[ofs]; ofs += 1;
//                 info.no_check_deck = payload[ofs] != 0; ofs += 1; info.no_shuffle_deck = payload[ofs] != 0; ofs += 4;
//                 info.start_lp = read_buffer_i32(payload, &mut ofs); info.start_hand = payload[ofs]; ofs += 1; info.draw_count = payload[ofs]; ofs += 1;
//                 info.time_limit = read_buffer_u16(payload, &mut ofs);
//                 if info.rule > ygopro_core_wrapper::CURRENT_RULE { info.rule = ygopro_core_wrapper::CURRENT_RULE; }
//                 if info.mode > 2 { info.mode = 0; }
//                 if !self.lflists.iter().any(|l| l.hash == info.lflist) { info.lflist = self.lflists.first().map(|l| l.hash).unwrap_or(0); }
//                 self.game_info = info;
//                 dm = match info.mode { 0 => Some(Box::new(single_duel::SingleDuel::new(false)) as Box<dyn DuelMode + Send>), 1 => Some(Box::new(single_duel::SingleDuel::new(true)) as Box<dyn DuelMode + Send>), 2 => Some(Box::new(tag_duel::TagDuel::new()) as Box<dyn DuelMode + Send>), _ => return };
//                 if let Some(ref mut d) = dm { *d.host_info_mut() = info; d.join_game(client_id, payload, true, self); }
//             }
//             CtosMsg::JoinGame => { if let Some(ref mut d) = dm { d.join_game(client_id, payload, false, self); } }
//             CtosMsg::LeaveGame => { if let Some(ref mut d) = dm { d.leave_game(client_id, self); } }
//             CtosMsg::Surrender => { if let Some(ref mut d) = dm { d.surrender(client_id, self); } }
//             CtosMsg::Chat => { if let Some(ref mut d) = dm { d.chat(client_id, payload, self); } }
//             CtosMsg::UpdateDeck => { if let Some(ref mut d) = dm { d.update_deck(client_id, payload, self); } }
//             CtosMsg::HandResult => { if let Some(ref mut d) = dm { if payload.len() >= 1 { d.hand_result(client_id, payload[0], self); } } }
//             CtosMsg::TpResult => { if let Some(ref mut d) = dm { if payload.len() >= 1 { d.tp_result(client_id, payload[0], self); } } }
//             CtosMsg::HsToDuelist => { if let Some(ref mut d) = dm { d.to_duelist(client_id, self); } }
//             CtosMsg::HsToObserver => { if let Some(ref mut d) = dm { d.to_observer(client_id, self); } }
//             CtosMsg::HsNotReady => { if let Some(ref mut d) = dm { d.player_ready(client_id, false, self); } }
//             CtosMsg::HsReady => { if let Some(ref mut d) = dm { d.player_ready(client_id, true, self); } }
//             CtosMsg::HsKick => { if let Some(ref mut d) = dm { if payload.len() >= 1 { d.player_kick(client_id, payload[0], self); } } }
//             CtosMsg::HsStart => { if let Some(ref mut d) = dm { d.start_duel(client_id, self); } }
//             CtosMsg::Response => { if let Some(ref mut d) = dm { d.get_response(client_id, payload, self); } }
//             CtosMsg::TimeConfirm => { if let Some(ref mut d) = dm { d.time_confirm(client_id, self); } }
//             CtosMsg::RequestField => { if let Some(ref mut d) = dm { d.request_field(client_id, self); } }
//             _ => {}
//         }
//         self.duel_mode = dm;
//     }

//     pub fn tick(&mut self) -> Vec<OutgoingMessage> { self.duel_mode.as_mut().map(|dm| dm.tick()).unwrap_or_default() }
//     pub fn drain_messages(&mut self) -> Vec<OutgoingMessage> { std::mem::take(&mut self.pending_messages) }
// }

// // 对应: ../ygopro/gframe/netserver.h:48-55 (SendPacketToPlayer)
// pub fn send_stoc_packet(srv: &mut YgoproServer, target: usize, proto: u8) {
//     srv.send_to(target, vec![1, 0, proto]);
// }

// // 对应: ../ygopro/gframe/netserver.h:67-77 (SendBufferToPlayer)
// pub fn send_stoc_buffer(srv: &mut YgoproServer, target: usize, proto: u8, data: &[u8]) {
//     let len = (1 + data.len()).min(MAX_DATA_SIZE);
//     let mut buf = Vec::with_capacity(len + 2);
//     buf.extend_from_slice(&(len as u16).to_le_bytes()); buf.push(proto); buf.extend_from_slice(data);
//     srv.send_to(target, buf);
// }
