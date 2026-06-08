// // 对应: ../ygopro/gframe/tag_duel.h, ../ygopro/gframe/tag_duel.cpp
// // YGOPRO_SERVER_MODE

// use std::collections::HashSet;
// use std::io::Cursor;

// use binrw::BinWrite;
// use rand::Rng;

// use ygopro_data::constants::Duelstage;
// use ygopro_data::constants::ErrorMessage;
// use ygopro_data::constants::Netplayer;
// use ygopro_data::data::check_deck;
// use ygopro_data::data::Deck;
// use ygopro_data::data::DeckErrorFlags;
// use ygopro_data::data::DuelOptions;
// use ygopro_data::data::encode_deck_error;
// use ygopro_data::message::stoc;
// use ygopro_data::message::stoc::MessageType as StocMsg;
// use ygopro_data_extend::network::*;

// use crate::constants::*;
// use crate::DuelMode;
// use crate::HostInfo;
// use crate::OutgoingMessage;
// use crate::OutgoingTarget;
// use crate::YgoproServer;

// // 对应: ../ygopro/gframe/tag_duel.h:11-87 (TagDuel)
// pub struct TagDuel {
//     pub host: HostInfo,
//     pub players: [Option<usize>; 4],
//     pub pplayers: [Option<usize>; 4],
//     pub cur_player: [Option<usize>; 2],
//     pub observers: HashSet<usize>,
//     pub cache_recorder: Option<usize>,
//     pub replay_recorder: Option<usize>,
//     pub ready: [bool; 4],
//     pub surrender: [bool; 4],
//     pub pdeck: [Deck; 4],
//     pub deck_error: [u32; 4],
//     pub hand_result: [u8; 2],
//     pub last_response: u8,
//     pub turn_count: u8,
//     pub time_limit: [i16; 2],
//     pub time_elapsed: i16,
//     pub duel_stage: u8,
//     pub pduel: Option<ygopro_core_wrapper::intptr_t>,
// }

// impl TagDuel {
//     // 对应: ../ygopro/gframe/tag_duel.cpp (TagDuel constructor)
//     pub fn new() -> Self {
//         Self {
//             host: HostInfo::default(), players: [None; 4], pplayers: [None; 4],
//             cur_player: [None; 2], observers: HashSet::new(),
//             cache_recorder: None, replay_recorder: None,
//             ready: [false; 4], surrender: [false; 4],
//             pdeck: Default::default(), deck_error: [0; 4],
//             hand_result: [0, 0], last_response: 0,
//             turn_count: 0, time_limit: [0, 0], time_elapsed: 0,
//             duel_stage: u8::from(Duelstage::Begin), pduel: None,
//         }
//     }

//     fn player_targets(&self) -> Vec<usize> { self.players.iter().flatten().copied().collect() }
//     fn all_targets(&self) -> Vec<usize> {
//         let mut v = self.player_targets();
//         v.extend(self.observers.iter().copied());
//         if let Some(r) = self.cache_recorder { v.push(r); }
//         if let Some(r) = self.replay_recorder { v.push(r); }
//         v
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (JoinGame)
//     fn join_game_impl(&mut self, client_id: usize, data: &[u8], is_creator: bool, srv: &mut YgoproServer) {
//         if !is_creator {
//             if let Some(c) = srv.get_client(client_id) { if c.client_type != 0xff { return; } }
//             if data.len() >= 6 && read_buffer_u16(data, &mut 0) != PRO_VERSION {
//                 send_stoc_msg(srv, client_id, &stoc::ErrorMessage { msg: ErrorMessage::VersionError, code: PRO_VERSION as u32 }, u8::from(StocMsg::ErrorMessage));
//                 return;
//             }
//         }
//         let join = host_info_to_wire(&self.host);
//         send_stoc_data(srv, client_id, u8::from(StocMsg::JoinGame), &join);
//         let slot = self.players.iter().position(|p| p.is_none());
//         if let Some(pos) = slot {
//             self.players[pos] = Some(client_id);
//             srv.get_client_mut(client_id).map(|c| c.client_type = pos as u8);
//         } else {
//             self.observers.insert(client_id);
//             srv.get_client_mut(client_id).map(|c| c.client_type = u8::from(Netplayer::Observer));
//         }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (LeaveGame)
//     fn leave_game_impl(&mut self, client_id: usize, _srv: &mut YgoproServer) {
//         self.observers.remove(&client_id);
//         for i in 0..4 { if self.players[i] == Some(client_id) { self.players[i] = None; self.ready[i] = false; } }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (ToDuelist)
//     fn to_duelist_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype != u8::from(Netplayer::Observer) { return; }
//         self.observers.remove(&client_id);
//         let slot = self.players.iter().position(|p| p.is_none());
//         if let Some(pos) = slot {
//             self.players[pos] = Some(client_id);
//             srv.get_client_mut(client_id).map(|c| c.client_type = pos as u8);
//             let name = srv.get_client(client_id).map(|c| c.name).unwrap_or([0; 20]);
//             let mut buf = to_bytes_vec(&name); buf.push(pos as u8);
//             for &p in self.players.iter().flatten() { send_stoc_data(srv, p, u8::from(StocMsg::HsPlayerEnter), &buf); }
//             let obs: Vec<usize> = self.observers.iter().copied().collect();
//             for &o in &obs { send_stoc_data(srv, o, u8::from(StocMsg::HsPlayerEnter), &buf); }
//         }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (ToObserver)
//     fn to_observer_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype > 3 { return; }
//         let status = (ptype << 4) | 0x8;
//         for &p in self.players.iter().flatten() { send_stoc_data(srv, p, u8::from(StocMsg::HsPlayerChange), &[status]); }
//         let obs: Vec<usize> = self.observers.iter().copied().collect();
//         for &o in &obs { send_stoc_data(srv, o, u8::from(StocMsg::HsPlayerChange), &[status]); }
//         self.players[ptype as usize] = None; self.ready[ptype as usize] = false;
//         self.observers.insert(client_id);
//         srv.get_client_mut(client_id).map(|c| c.client_type = u8::from(Netplayer::Observer));
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (PlayerReady)
//     fn player_ready_impl(&mut self, client_id: usize, is_ready: bool, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff) as usize;
//         if ptype > 3 { return; }
//         if self.ready[ptype] == is_ready { return; }
//         if is_ready && !self.host.no_check_deck {
//             let deckerror = if self.deck_error[ptype] != 0 {
//                 (DeckErrorFlags::UNKNOWNCARD as u32 << 28) | self.deck_error[ptype]
//             } else {
//                 let lflist_map = srv.lflists.iter().find(|l| l.hash == self.host.lflist).map(|l| l.content.clone()).unwrap_or_default();
//                 match check_deck(&self.pdeck[ptype].main, &self.pdeck[ptype].extra, &self.pdeck[ptype].side, &lflist_map, |code| {
//                     srv.data_manager.datas.get(&code).map(|d| d.duel_code()).unwrap_or(code)
//                 }) { Ok(()) => 0, Err(e) => encode_deck_error(e.flags, e.code) }
//             };
//             if deckerror != 0 {
//                 send_stoc_msg(srv, client_id, &stoc::ErrorMessage { msg: ErrorMessage::DeckError, code: deckerror }, u8::from(StocMsg::ErrorMessage));
//                 return;
//             }
//         }
//         self.ready[ptype] = is_ready;
//         let status = (ptype as u8) << 4 | if is_ready { 0x9 } else { 0xa };
//         for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::HsPlayerChange), &[status]); }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (PlayerKick)
//     fn player_kick_impl(&mut self, _: usize, pos: u8, srv: &mut YgoproServer) {
//         if pos > 3 { return; }
//         if let Some(kicked) = self.players[pos as usize] { self.leave_game_impl(kicked, srv); }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (UpdateDeck)
//     fn update_deck_impl(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff) as usize;
//         if ptype > 3 || self.ready[ptype] || data.len() < 8 { return; }
//         let mainc = read_buffer_i32(data, &mut 0) as usize;
//         let sidec = read_buffer_i32(data, &mut 4) as usize;
//         if mainc > MAINC_MAX || sidec > SIDEC_MAX { return; }
//         let mut codes = Vec::new(); let mut ofs = 8;
//         for _ in 0..(mainc + sidec) { codes.push(read_buffer_u32(data, &mut ofs)); }
//         self.pdeck[ptype] = Deck::load_from_codes(&codes, mainc, sidec);
//         self.deck_error[ptype] = 0;
//         self.player_ready_impl(client_id, true, srv);
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (StartDuel)
//     fn start_duel_impl(&mut self, _cid: usize, srv: &mut YgoproServer) {
//         if self.ready.iter().filter(|&&r| r).count() < 4 { return; }
//         for &p in self.players.iter().flatten() { send_stoc_packet(srv, p, u8::from(StocMsg::DuelStart)); }
//         self.duel_stage = u8::from(Duelstage::Finger);
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (HandResult)
//     fn hand_result_impl(&mut self, _client_id: usize, _res: u8, _srv: &mut YgoproServer) {
//         // Tag mode: RPS between team leaders (players 0 and 2)
//         // Simplified: auto-pass
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (TPResult)
//     fn tp_result_impl(&mut self, _client_id: usize, _tp: u8, srv: &mut YgoproServer) {
//         self.duel_stage = u8::from(Duelstage::Dueling);
//         let mut seeds = [0u32; ygopro_core_wrapper::SEED_COUNT];
//         for s in seeds.iter_mut() { *s = rand::thread_rng().gen(); }
//         let pduel = ygopro_core_wrapper::create_duel_safe(&seeds); self.pduel = Some(pduel);
//         unsafe {
//             ygopro_core_wrapper::set_player_info(pduel, 0, self.host.start_lp, self.host.start_hand as i32, self.host.draw_count as i32);
//             ygopro_core_wrapper::set_player_info(pduel, 1, self.host.start_lp, self.host.start_hand as i32, self.host.draw_count as i32);
//         }
//         let opts = (DuelOptions::TagMode.bits() | DuelOptions::PseudoShuffle.bits()) as u32 | ((self.host.duel_rule as u32) << 16);
//         unsafe { ygopro_core_wrapper::start_duel(pduel, opts); }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (Chat)
//     fn chat_impl(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer) {
//         let ct = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(7) as u16;
//         let mut scc = vec![ct as u8, (ct >> 8) as u8];
//         scc.extend_from_slice(&data[..data.len().min(LEN_CHAT_MSG * 2)]);
//         for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::Chat), &scc); }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (GetResponse)
//     fn get_response_impl(&mut self, _cid: usize, data: &[u8], srv: &mut YgoproServer) {
//         if let Some(pduel) = self.pduel { unsafe { ygopro_core_wrapper::set_responseb(pduel, data.as_ptr() as *mut u8); } self.process_impl(srv); }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (Process)
//     fn process_impl(&mut self, srv: &mut YgoproServer) {
//         let pduel = match self.pduel { Some(p) => p, None => return };
//         let mut stop = 0i32;
//         loop {
//             if stop != 0 { break; }
//             let result = unsafe { ygopro_core_wrapper::process(pduel) };
//             let eng_len = result & ygopro_core_wrapper::PROCESSOR_BUFFER_LEN;
//             let eng_flag = result & ygopro_core_wrapper::PROCESSOR_FLAG;
//             if eng_flag == ygopro_core_wrapper::PROCESSOR_END { break; }
//             if eng_len > 0 {
//                 let mut buf = vec![0u8; eng_len as usize];
//                 unsafe { ygopro_core_wrapper::get_message(pduel, buf.as_mut_ptr()); }
//                 stop = self.analyze_impl(&buf, srv);
//             }
//         }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (Analyze)
//     fn analyze_impl(&mut self, buf: &[u8], srv: &mut YgoproServer) -> i32 {
//         let mut ofs = 0; let len = buf.len();
//         while ofs < len {
//             let start = ofs;
//             let eng = read_buffer_u8(buf, &mut ofs);
//             match eng {
//                 1 => { for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } return 1; }
//                 5 => { ofs += 2; for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } return 2; }
//                 10|11|12|13|14|15|16|18|19|20|22|23|24|25|26 => {
//                     let _player = read_buffer_u8(buf, &mut ofs);
//                     for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                     return 1;
//                 }
//                 40|41|50|53|54|32|35|37|38|30|42|31|33|36|39|161 => {
//                     for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 _ => { ofs = len; }
//             }
//         }
//         0
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (Surrender)
//     fn surrender_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype > 3 || self.pduel.is_none() { return; }
//         self.surrender[ptype as usize] = true;
//         let wbuf = [5, (1 - (ptype & 1)) as u8, 0u8];
//         for t in self.all_targets() { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &wbuf); }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (EndDuel)
//     fn end_duel_impl(&mut self, _: &mut YgoproServer) {
//         if let Some(pduel) = self.pduel { unsafe { ygopro_core_wrapper::end_duel(pduel); } self.pduel = None; }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (TimeConfirm)
//     fn time_confirm_impl(&mut self, _: usize, _: &mut YgoproServer) {}

//     // 对应: ../ygopro/gframe/tag_duel.cpp (RequestField)
//     fn request_field_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         if let Some(pduel) = self.pduel {
//             let mut buf = vec![0u8; SIZE_QUERY_BUFFER_SERVER];
//             let len = unsafe { ygopro_core_wrapper::query_field_info(pduel, buf.as_mut_ptr()) };
//             if len > 0 { buf.truncate(len as usize); send_stoc_data(srv, client_id, u8::from(StocMsg::FieldFinish), &buf); }
//         }
//     }

//     // 对应: ../ygopro/gframe/tag_duel.cpp (Tick)
//     fn tick_impl(&mut self) -> Vec<OutgoingMessage> { Vec::new() }
// }

// // ============== DuelMode trait impl ==============
// impl DuelMode for TagDuel {
//     fn host_info(&self) -> HostInfo { self.host }
//     fn host_info_mut(&mut self) -> &mut HostInfo { &mut self.host }
//     fn pduel(&self) -> Option<ygopro_core_wrapper::intptr_t> { self.pduel }
//     fn duel_stage(&self) -> u8 { self.duel_stage }
//     fn join_game(&mut self, c: usize, d: &[u8], i: bool, s: &mut YgoproServer) { self.join_game_impl(c, d, i, s); }
//     fn leave_game(&mut self, c: usize, s: &mut YgoproServer) { self.leave_game_impl(c, s); }
//     fn to_duelist(&mut self, c: usize, s: &mut YgoproServer) { self.to_duelist_impl(c, s); }
//     fn to_observer(&mut self, c: usize, s: &mut YgoproServer) { self.to_observer_impl(c, s); }
//     fn player_ready(&mut self, c: usize, i: bool, s: &mut YgoproServer) { self.player_ready_impl(c, i, s); }
//     fn player_kick(&mut self, c: usize, p: u8, s: &mut YgoproServer) { self.player_kick_impl(c, p, s); }
//     fn update_deck(&mut self, c: usize, d: &[u8], s: &mut YgoproServer) { self.update_deck_impl(c, d, s); }
//     fn start_duel(&mut self, c: usize, s: &mut YgoproServer) { self.start_duel_impl(c, s); }
//     fn hand_result(&mut self, c: usize, r: u8, s: &mut YgoproServer) { self.hand_result_impl(c, r, s); }
//     fn tp_result(&mut self, c: usize, t: u8, s: &mut YgoproServer) { self.tp_result_impl(c, t, s); }
//     fn chat(&mut self, c: usize, d: &[u8], s: &mut YgoproServer) { self.chat_impl(c, d, s); }
//     fn get_response(&mut self, c: usize, d: &[u8], s: &mut YgoproServer) { self.get_response_impl(c, d, s); }
//     fn time_confirm(&mut self, c: usize, s: &mut YgoproServer) { self.time_confirm_impl(c, s); }
//     fn surrender(&mut self, c: usize, s: &mut YgoproServer) { self.surrender_impl(c, s); }
//     fn request_field(&mut self, c: usize, s: &mut YgoproServer) { self.request_field_impl(c, s); }
//     fn process(&mut self, s: &mut YgoproServer) { self.process_impl(s); }
//     fn end_duel(&mut self, s: &mut YgoproServer) { self.end_duel_impl(s); }
//     fn tick(&mut self) -> Vec<OutgoingMessage> { self.tick_impl() }
//     fn analyze(&mut self, b: &[u8], s: &mut YgoproServer) -> i32 { self.analyze_impl(b, s) }
// }

// // ============== Helpers ==============
// fn to_bytes_vec<T: Sized>(val: &T) -> Vec<u8> {
//     unsafe { std::slice::from_raw_parts(val as *const T as *const u8, std::mem::size_of::<T>()).to_vec() }
// }

// fn send_stoc_packet(srv: &mut YgoproServer, target: usize, proto: u8) { srv.send_to(target, vec![1, 0, proto]); }

// fn send_stoc_data(srv: &mut YgoproServer, target: usize, proto: u8, data: &[u8]) {
//     let len = (1 + data.len()).min(MAX_DATA_SIZE);
//     let mut buf = vec![];
//     buf.extend_from_slice(&(len as u16).to_le_bytes()); buf.push(proto); buf.extend_from_slice(data);
//     srv.send_to(target, buf);
// }

// fn send_stoc_msg<T: BinWrite>(srv: &mut YgoproServer, target: usize, msg: &T, proto: u8) where for<'a> <T as BinWrite>::Args<'a>: Default {
//     let mut buf = Cursor::new(Vec::new()); msg.write_le(&mut buf).ok();
//     send_stoc_data(srv, target, proto, buf.get_ref());
// }

// fn host_info_to_wire(info: &HostInfo) -> Vec<u8> {
//     let mut buf = vec![];
//     buf.extend_from_slice(&info.lflist.to_le_bytes());
//     buf.push(info.rule); buf.push(info.mode); buf.push(info.duel_rule);
//     buf.push(info.no_check_deck as u8); buf.push(info.no_shuffle_deck as u8);
//     buf.extend_from_slice(&[0u8; 3]);
//     buf.extend_from_slice(&info.start_lp.to_le_bytes());
//     buf.push(info.start_hand); buf.push(info.draw_count);
//     buf.extend_from_slice(&info.time_limit.to_le_bytes());
//     buf
// }
