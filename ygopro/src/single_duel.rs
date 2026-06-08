// // 对应: ../ygopro/gframe/single_duel.h, ../ygopro/gframe/single_duel.cpp
// // YGOPRO_SERVER_MODE

// use std::collections::HashSet;
// use std::io::Cursor;

// use binrw::BinWrite;
// use rand::Rng;

// use ygopro_core_wrapper as core;
// use ygopro_data::constants::Duelstage;
// use ygopro_data::constants::ErrorMessage;
// use ygopro_data::constants::GameMessage;
// use ygopro_data::constants::Hand;
// use ygopro_data::constants::Location;
// use ygopro_data::constants::Netplayer;
// use ygopro_data::constants::PlayerChange;
// use ygopro_data::constants::Position;
// use ygopro_data::data::check_deck;
// use ygopro_data::data::Deck;
// use ygopro_data::data::DeckErrorFlags;
// use ygopro_data::data::DuelOptions;
// use ygopro_data::data::encode_deck_error;
// use ygopro_data::data::ReplayData;
// use ygopro_data::data::side_from_codes;
// use ygopro_data::message::ctos::MessageType as CtosMsg;
// use ygopro_data::message::stoc;
// use ygopro_data::message::stoc::MessageType as StocMsg;
// use ygopro_data_extend::network::*;
// use ygopro_data_extend::random::MTRandom;

// use crate::constants::*;
// use crate::DuelMode;
// use crate::HostInfo;
// use crate::OutgoingMessage;
// use crate::OutgoingTarget;
// use crate::YgoproServer;

// pub struct SingleDuel {
//     pub match_mode: bool,
//     pub host: HostInfo,
//     pub players: [Option<usize>; 2],
//     pub pplayers: [Option<usize>; 2],
//     pub ready: [bool; 2],
//     pub pdeck: [Deck; 2],
//     pub deck_error: [u32; 2],
//     pub hand_result: [u8; 2],
//     pub last_response: u8,
//     pub observers: HashSet<usize>,
//     pub cache_recorder: Option<usize>,
//     pub replay_recorder: Option<usize>,
//     pub turn_player: u8, pub phase: u16, pub deck_reversed: bool,
//     pub replay_datas: Vec<ReplayData>,
//     pub match_kill: u8, pub duel_count: u8, pub tp_player: u8,
//     pub match_result: [u8; 3],
//     pub time_limit: [i16; 2], pub time_elapsed: i16,
//     pub time_compensator: [i16; 2], pub time_backed: [i16; 2],
//     pub last_game_msg: u8, pub duel_stage: u8,
//     pub pduel: Option<core::intptr_t>,
// }

// impl SingleDuel {
//     pub fn new(is_match: bool) -> Self {
//         Self {
//             match_mode: is_match, host: HostInfo::default(),
//             players: [None, None], pplayers: [None, None],
//             ready: [false, false],
//             pdeck: [Deck::new(), Deck::new()], deck_error: [0, 0],
//             hand_result: [0, 0], last_response: 0,
//             observers: HashSet::new(),
//             cache_recorder: None, replay_recorder: None,
//             turn_player: 0, phase: 0, deck_reversed: false,
//             replay_datas: Vec::new(),
//             match_kill: 0, duel_count: 0, tp_player: 0,
//             match_result: [0; 3],
//             time_limit: [0, 0], time_elapsed: 0,
//             time_compensator: [0, 0], time_backed: [0, 0],
//             last_game_msg: 0, duel_stage: u8::from(Duelstage::Begin),
//             pduel: None,
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

//     // ============== Chat ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:18-33
//     fn chat_impl(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer) {
//         let src_len = data.len().min(LEN_CHAT_MSG * 2);
//         let ct = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(7) as u16;
//         let mut scc = vec![ct as u8, (ct >> 8) as u8];
//         if src_len >= 2 { scc.extend_from_slice(&data[..src_len]); }
//         let targets = self.all_targets();
//         for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::Chat), &scc); }
//     }

//     // ============== PlayerReady ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:343-382
//     fn player_ready_impl(&mut self, client_id: usize, is_ready: bool, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff) as usize;
//         if ptype > 1 { return; }
//         if self.ready[ptype] == is_ready { return; }

//         if is_ready {
//             let mut deckerror = 0u32;
//             if !self.host.no_check_deck {
//                 if self.deck_error[ptype] != 0 {
//                     deckerror = (DeckErrorFlags::UNKNOWNCARD as u32 << 28) | self.deck_error[ptype];
//                 } else {
//                     let lflist_map = srv.lflists.iter().find(|l| l.hash == self.host.lflist).map(|l| l.content.clone()).unwrap_or_default();
//                     deckerror = match check_deck(&self.pdeck[ptype].main, &self.pdeck[ptype].extra, &self.pdeck[ptype].side, &lflist_map, |code| {
//                         srv.data_manager.datas.get(&code).map(|d| d.duel_code()).unwrap_or(code)
//                     }) { Ok(()) => 0, Err(e) => encode_deck_error(e.flags, e.code) };
//                 }
//             }
//             if deckerror != 0 {
//                 let pc = PlayerChange::Notready(Netplayer::try_from(ptype as u8).unwrap_or(Netplayer::Player1));
//                 send_stoc_msg(srv, client_id, &stoc::HsPlayerChange { status: pc }, u8::from(StocMsg::HsPlayerChange));
//                 send_stoc_msg(srv, client_id, &stoc::ErrorMessage { msg: ErrorMessage::DeckError, code: deckerror }, u8::from(StocMsg::ErrorMessage));
//                 return;
//             }
//         }

//         self.ready[ptype] = is_ready;
//         let pc = if is_ready { PlayerChange::Ready(Netplayer::try_from(ptype as u8).unwrap_or(Netplayer::Player1)) } else { PlayerChange::Notready(Netplayer::try_from(ptype as u8).unwrap_or(Netplayer::Player1)) };
//         let msg = stoc::HsPlayerChange { status: pc }; let proto = u8::from(StocMsg::HsPlayerChange);
//         for t in self.all_targets() { send_stoc_msg(srv, t, &msg, proto); }
//     }

//     // ============== UpdateDeck ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:388-438
//     fn update_deck_impl(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff) as usize;
//         if ptype > 1 || self.ready[ptype] { return; }
//         if data.len() < 8 || data.len() > 2008 { return; }
//         let mainc = read_buffer_i32(data, &mut 0);
//         let sidec = read_buffer_i32(data, &mut 4);
//         if mainc < 0 || mainc > MAINC_MAX as i32 || sidec < 0 || sidec > SIDEC_MAX as i32 { return; }
//         let total = (mainc + sidec) as usize;
//         if data.len() < 8 + total * 4 { return; }
//         let mut codes = Vec::with_capacity(total);
//         let mut ofs = 8;
//         for _ in 0..total { codes.push(read_buffer_u32(data, &mut ofs)); }

//         if self.duel_count == 0 {
//             self.pdeck[ptype] = Deck::load_from_codes(&codes, mainc as usize, sidec as usize);
//             self.deck_error[ptype] = 0;
//             self.player_ready_impl(client_id, true, srv);
//         } else {
//             if side_from_codes(&mut self.pdeck[ptype], &codes, mainc as usize, sidec as usize) {
//                 self.ready[ptype] = true;
//                 send_stoc_packet(srv, client_id, u8::from(StocMsg::DuelStart));
//                 if self.ready[0] && self.ready[1] {
//                     send_stoc_packet(srv, self.players[self.tp_player as usize].unwrap(), u8::from(StocMsg::SelectTp));
//                     let other = self.players[1 - self.tp_player as usize].unwrap();
//                     srv.get_client_mut(other).map(|c| c.state = 0xff);
//                     srv.get_client_mut(client_id).map(|c| c.state = u8::from(CtosMsg::TpResult));
//                     self.duel_stage = u8::from(Duelstage::Firstgo);
//                 }
//             } else {
//                 send_stoc_msg(srv, client_id, &stoc::ErrorMessage { msg: ErrorMessage::SideError, code: 0 }, u8::from(StocMsg::ErrorMessage));
//             }
//         }
//     }

//     // ============== StartDuel ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:439-480
//     fn start_duel_impl(&mut self, _cid: usize, srv: &mut YgoproServer) {
//         if !self.ready[0] || !self.ready[1] { return; }

//         let ptargets = self.player_targets();
//         for &p in &ptargets { send_stoc_packet(srv, p, u8::from(StocMsg::DuelStart)); }
//         let otargets: Vec<usize> = self.observers.iter().copied().collect();
//         for &o in &otargets {
//             srv.get_client_mut(o).map(|c| c.state = u8::from(CtosMsg::LeaveGame));
//             send_stoc_packet(srv, o, u8::from(StocMsg::DuelStart));
//         }

//         let dc0 = stoc::DeckCount {
//             mainc_s: self.pdeck[0].main.len() as u16, sidec_s: self.pdeck[0].side.len() as u16, extrac_s: self.pdeck[0].extra.len() as u16,
//             mainc_o: self.pdeck[1].main.len() as u16, sidec_o: self.pdeck[1].side.len() as u16, extrac_o: self.pdeck[1].extra.len() as u16,
//         };
//         send_stoc_msg(srv, self.players[0].unwrap(), &dc0, u8::from(StocMsg::DeckCount));
//         let dc1 = stoc::DeckCount {
//             mainc_s: dc0.mainc_o, sidec_s: dc0.sidec_o, extrac_s: dc0.extrac_o,
//             mainc_o: dc0.mainc_s, sidec_o: dc0.sidec_s, extrac_o: dc0.extrac_s,
//         };
//         send_stoc_msg(srv, self.players[1].unwrap(), &dc1, u8::from(StocMsg::DeckCount));

//         for &p in &ptargets { send_stoc_packet(srv, p, u8::from(StocMsg::SelectHand)); }
//         self.hand_result = [0, 0];
//         for &p in &ptargets { srv.get_client_mut(p).map(|c| c.state = u8::from(CtosMsg::HandResult)); }
//         self.duel_stage = u8::from(Duelstage::Finger);
//     }

//     // ============== HandResult ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:481-523
//     fn hand_result_impl(&mut self, client_id: usize, res: u8, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff) as usize;
//         if res > 3 || ptype > 1 { return; }
//         if srv.get_client(client_id).map(|c| c.state).unwrap_or(0) != u8::from(CtosMsg::HandResult) { return; }
//         self.hand_result[ptype] = res;

//         if self.hand_result[0] != 0 && self.hand_result[1] != 0 {
//             let p0 = self.players[0].unwrap(); let p1 = self.players[1].unwrap();
//             let obs: Vec<usize> = self.observers.iter().copied().collect();

//             let res1 = Hand::try_from(self.hand_result[0]).unwrap_or(Hand::Rock);
//             let res2 = Hand::try_from(self.hand_result[1]).unwrap_or(Hand::Rock);
//             let hr = stoc::HandResult { res1, res2 };
//             send_stoc_msg(srv, p0, &hr, u8::from(StocMsg::HandResult));
//             for &o in &obs { send_stoc_msg(srv, o, &hr, u8::from(StocMsg::HandResult)); }
//             if let Some(r) = self.cache_recorder { send_stoc_msg(srv, r, &hr, u8::from(StocMsg::HandResult)); }
//             if let Some(r) = self.replay_recorder { send_stoc_msg(srv, r, &hr, u8::from(StocMsg::HandResult)); }
//             let hr_swapped = stoc::HandResult { res1: res2, res2: res1 };
//             send_stoc_msg(srv, p1, &hr_swapped, u8::from(StocMsg::HandResult));

//             let r0 = self.hand_result[0] as i32; let r1 = self.hand_result[1] as i32;
//             if r0 == r1 {
//                 send_stoc_packet(srv, p0, u8::from(StocMsg::SelectHand));
//                 send_stoc_packet(srv, p1, u8::from(StocMsg::SelectHand));
//                 self.hand_result = [0, 0];
//                 srv.get_client_mut(p0).map(|c| c.state = u8::from(CtosMsg::HandResult));
//                 srv.get_client_mut(p1).map(|c| c.state = u8::from(CtosMsg::HandResult));
//             } else if (r0 == 1 && r1 == 2) || (r0 == 2 && r1 == 3) || (r0 == 3 && r1 == 1) {
//                 send_stoc_packet(srv, p1, u8::from(StocMsg::SelectTp)); self.tp_player = 1;
//                 srv.get_client_mut(p0).map(|c| c.state = 0xff);
//                 srv.get_client_mut(p1).map(|c| c.state = u8::from(CtosMsg::TpResult));
//                 self.duel_stage = u8::from(Duelstage::Firstgo);
//             } else {
//                 send_stoc_packet(srv, p0, u8::from(StocMsg::SelectTp)); self.tp_player = 0;
//                 srv.get_client_mut(p1).map(|c| c.state = 0xff);
//                 srv.get_client_mut(p0).map(|c| c.state = u8::from(CtosMsg::TpResult));
//                 self.duel_stage = u8::from(Duelstage::Firstgo);
//             }
//         }
//     }

//     // ============== TPResult ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:524-637
//     fn tp_result_impl(&mut self, client_id: usize, tp: u8, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff) as usize;
//         if srv.get_client(client_id).map(|c| c.state).unwrap_or(0) != u8::from(CtosMsg::TpResult) { return; }
//         self.duel_stage = u8::from(Duelstage::Dueling); self.pplayers = self.players;

//         let mut swapped = false;
//         if (tp != 0 && ptype == 1) || (tp == 0 && ptype == 0) {
//             self.players.swap(0, 1);
//             if let Some(p) = self.players[0] { srv.get_client_mut(p).map(|c| c.client_type = 0); }
//             if let Some(p) = self.players[1] { srv.get_client_mut(p).map(|c| c.client_type = 1); }
//             self.pdeck.swap(0, 1); swapped = true;
//         }
//         srv.get_client_mut(client_id).map(|c| c.state = u8::from(CtosMsg::Response));

//         let mut seeds = [0u32; core::SEED_COUNT];
//         if srv.pre_seed_specified[self.duel_count as usize] != 0 {
//             seeds.copy_from_slice(&srv.pre_seed[self.duel_count as usize]);
//         } else { for s in seeds.iter_mut() { *s = rand::thread_rng().gen(); } }
//         let mut rnd = MTRandom::new(&seeds, core::SEED_COUNT);

//         if !self.host.no_shuffle_deck { rnd.shuffle_vector(&mut self.pdeck[0].main); rnd.shuffle_vector(&mut self.pdeck[1].main); }
//         self.time_limit = [self.host.time_limit as i16, self.host.time_limit as i16];

//         unsafe {
//             core::set_script_reader(Some(crate::script_reader_callback));
//             core::set_card_reader(Some(crate::card_reader_callback));
//             core::set_message_handler(Some(Self::message_handler_raw));
//         }
//         let pduel = core::create_duel_safe(&seeds); self.pduel = Some(pduel);
//         unsafe {
//             core::set_player_info(pduel, 0, self.host.start_lp, self.host.start_hand as i32, self.host.draw_count as i32);
//             core::set_player_info(pduel, 1, self.host.start_lp, self.host.start_hand as i32, self.host.draw_count as i32);
//         }
//         let _ = core::preload_script_from_path(pduel, "./script/special.lua");

//         let duel_opts = if self.host.no_shuffle_deck { DuelOptions::PseudoShuffle.bits() } else { 0u16 };
//         let opt = ((self.host.duel_rule as u32) << 16) | duel_opts as u32;

//         let load = |deck: &[u32], player: u8, location: u8| {
//             for &code in deck.iter().rev() { unsafe { core::new_card(pduel, code, player, player, location, 0, Position::FacedownDefense as u8); } }
//         };
//         load(&self.pdeck[0].main.clone(), 0, Location::Deck.bits());
//         load(&self.pdeck[0].extra.clone(), 0, Location::Extra.bits());
//         load(&self.pdeck[1].main.clone(), 1, Location::Deck.bits());
//         load(&self.pdeck[1].extra.clone(), 1, Location::Extra.bits());

//         self.replay_datas = Vec::new();
//         let p0 = self.players[0].unwrap(); let p1 = self.players[1].unwrap();
//         let startbuf = build_start_msg(pduel, self.host.duel_rule, self.host.start_lp);
//         send_stoc_data(srv, p0, u8::from(StocMsg::GameMessage), &startbuf);
//         let mut s1 = startbuf.clone(); s1[1] = 1;
//         send_stoc_data(srv, p1, u8::from(StocMsg::GameMessage), &s1);
//         let mut sobs = startbuf; sobs[1] = if !swapped { 0x10 } else { 0x11 };
//         let obs: Vec<usize> = self.observers.iter().copied().collect();
//         for &o in &obs { send_stoc_data(srv, o, u8::from(StocMsg::GameMessage), &sobs); }
//         if let Some(r) = self.cache_recorder { send_stoc_data(srv, r, u8::from(StocMsg::GameMessage), &sobs); }
//         if let Some(r) = self.replay_recorder { send_stoc_data(srv, r, u8::from(StocMsg::GameMessage), &sobs); }

//         self.turn_player = 0; self.phase = 1; self.deck_reversed = false;
//         unsafe { core::start_duel(pduel, opt); }
//         self.time_elapsed = 0;
//         self.time_compensator = [self.host.time_limit as i16, self.host.time_limit as i16];
//         self.time_backed = [self.host.time_limit as i16, self.host.time_limit as i16];
//         self.last_game_msg = 0;
//         self.process_impl(srv);
//     }

//     // ============== GetResponse ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp (GetResponse)
//     fn get_response_impl(&mut self, client_id: usize, data: &[u8], srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype > 1 || data.is_empty() { return; }
//         if let Some(pduel) = self.pduel { unsafe { core::set_responseb(pduel, data.as_ptr() as *mut u8); } self.process_impl(srv); }
//     }

//     // ============== Process / Analyze ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:638-659
//     fn process_impl(&mut self, srv: &mut YgoproServer) {
//         let pduel = match self.pduel { Some(p) => p, None => return };
//         let mut stop = 0i32;
//         loop {
//             if stop != 0 { break; }
//             let result = unsafe { core::process(pduel) };
//             let eng_len = result & core::PROCESSOR_BUFFER_LEN;
//             let eng_flag = result & core::PROCESSOR_FLAG;
//             if eng_flag == core::PROCESSOR_END { break; }
//             if eng_len > 0 {
//                 let mut buf = vec![0u8; eng_len as usize];
//                 unsafe { core::get_message(pduel, buf.as_mut_ptr()); }
//                 stop = self.analyze_impl(&buf, srv);
//             }
//         }
//         if stop == 2 { self.duel_end_proc(srv); }
//     }

//     /// 对应: ../ygopro/gframe/single_duel.cpp:745-1832 (Analyze)
//     fn analyze_impl(&mut self, buf: &[u8], srv: &mut YgoproServer) -> i32 {
//         let mut ofs = 0; let len = buf.len();
//         while ofs < len {
//             let start = ofs;
//             let eng = read_buffer_u8(buf, &mut ofs); self.last_game_msg = eng;
//             match eng {
//                 // MSG_RETRY - 对应 analyze line 755-759
//                 1 => { self.wait_for_response(self.last_response, srv); send_stoc_data(srv, self.players[self.last_response as usize].unwrap(), u8::from(StocMsg::GameMessage), &buf[start..ofs]); return 1; }
//                 // MSG_HINT - 对应 analyze line 760-798
//                 2 => { let ht = read_buffer_u8(buf, &mut ofs); let p = read_buffer_u8(buf, &mut ofs); let _ = read_buffer_i32(buf, &mut ofs);
//                     match ht { 1|2|3|5 => send_stoc_data(srv, self.players[p as usize].unwrap(), u8::from(StocMsg::GameMessage), &buf[start..ofs]),
//                         _ => { let o = self.players[1 - p as usize].unwrap(); send_stoc_data(srv, o, u8::from(StocMsg::GameMessage), &buf[start..ofs]);
//                             let targets = self.all_targets(); for t in targets { if t != self.players[p as usize].unwrap() { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                         }
//                     }
//                 }
//                 // MSG_WAITING - 对应 analyze message 3
//                 3 => { let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_START - already handled in TPResult, skip here
//                 4 => { ofs = len; }
//                 // MSG_WIN - 对应 analyze line 799-821
//                 5 => { let player = read_buffer_u8(buf, &mut ofs); let _wt = read_buffer_u8(buf, &mut ofs);
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                     if player > 1 { self.match_result[self.duel_count as usize] = 2; self.tp_player = 1 - self.tp_player; }
//                     else if self.players[player as usize] == self.pplayers[player as usize] { self.match_result[self.duel_count as usize] = player; self.tp_player = 1 - player; }
//                     else { self.match_result[self.duel_count as usize] = 1 - player; self.tp_player = player; }
//                     self.duel_count += 1; self.end_duel_impl(srv); return 2;
//                 }
//                 // MSG_UPDATE_DATA + MSG_UPDATE_CARD - 对应 analyze message 6,7
//                 6|7 => { ofs = len; }
//                 // MSG_REQUEST_DECK - 对应 analyze message 8
//                 8 => { let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SELECT_BATTLECMD/IDLECMD/EFFECTYN/YESNO/OPTION/CARD/TRIBUTE/CHAIN/PLACE/DISFIELD/POSITION/COUNTER/SUM/SORT/UNSELECT
//                 // 对应 analyze line 822-982
//                 10|11|12|13|14|15|16|18|19|20|22|23|24|25|26 => {
//                     let player = read_buffer_u8(buf, &mut ofs); ofs = len;
//                     self.wait_for_response(player, srv);
//                     send_stoc_data(srv, self.players[player as usize].unwrap(), u8::from(StocMsg::GameMessage), &buf[start..ofs]);
//                     if matches!(eng, 10|11) { self.refresh_all(srv); }
//                     return 1;
//                 }
//                 // MSG_CONFIRM_DECKTOP/EXTRATOP/CARDS - 对应 analyze line 983-1027
//                 30|31|42 => {
//                     let player = read_buffer_u8(buf, &mut ofs); ofs = len;
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 // MSG_SHUFFLE_DECK - 对应 analyze line 1028-1038
//                 32 => { let _ = read_buffer_u8(buf, &mut ofs); ofs = len;
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 // MSG_SHUFFLE_HAND - 对应 analyze line 1039-1056
//                 33 => { self.refresh_hand(read_buffer_u8(buf, &mut ofs) as u32, srv); ofs = len; }
//                 // MSG_REFRESH_DECK - 对应 analyze line 1075-1085
//                 34 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SWAP_GRAVE_DECK - 对应 analyze line 1086-1097
//                 35 => { let player = read_buffer_u8(buf, &mut ofs); ofs = len;
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 // MSG_SHUFFLE_SET_CARD - 对应 analyze line 1120-1140
//                 36 => { let _loc = read_buffer_u8(buf, &mut ofs); ofs = len;
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                     self.refresh_all(srv);
//                 }
//                 // MSG_REVERSE_DECK - 对应 analyze line 1098-1108
//                 37 => { ofs = len; self.deck_reversed = !self.deck_reversed;
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 // MSG_DECK_TOP - 对应 analyze line 1109-1119
//                 38 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SHUFFLE_EXTRA - 对应 analyze line 1057-1074
//                 39 => { let player = read_buffer_u8(buf, &mut ofs); ofs = len; self.refresh_extra(player as u32, srv); }
//                 // MSG_NEW_TURN - 对应 analyze line 1141-1169
//                 40 => { self.turn_player = read_buffer_u8(buf, &mut ofs); ofs = len; self.time_limit = [self.host.time_limit as i16, self.host.time_limit as i16];
//                     self.refresh_all(srv);
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 // MSG_NEW_PHASE - 对应 analyze line 1170-1190
//                 41 => { self.phase = read_buffer_u16(buf, &mut ofs); ofs = len;
//                     self.refresh_all(srv);
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                 }
//                 // MSG_MOVE/POS_CHANGE/SET/SWAP/FIELD_DISABLED - 对应 analyze line 1191-1274
//                 50|53|54|55|56 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SUMMONING - 对应 analyze line 1275-1285
//                 60 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SUMMONED - 对应 analyze line 1286-1299
//                 61 => { ofs = len; self.refresh_all(srv); let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SPSUMMONING - 对应 analyze line 1300-1320
//                 62 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_SPSUMMONED/FLIPSUMMONED - 对应 analyze line 1321-1360
//                 63|65 => { ofs = len; self.refresh_all(srv); let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_FLIPSUMMONING - 对应 analyze line 1335-1346
//                 64 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_CHAINING/CHAINED/SOLVING/SOLVED/END/NEGATED/DISABLED - 对应 analyze line 1361-1454
//                 70|71|72|73|74|75|76 => { ofs = len;
//                     let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); }
//                     if matches!(eng, 71|73|74) { self.refresh_all(srv); }
//                 }
//                 // MSG_CARD_SELECTED/RANDOM_SELECTED/BECOME_TARGET - 对应 analyze line 1455-1484
//                 80|81|83 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_DRAW/DAMAGE/RECOVER - 对应 analyze line 1486-1530
//                 90|91|92 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_EQUIP/UNEQUIP/CARD_TARGET/CANCEL_TARGET - 对应 analyze
//                 93|95|96|97 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_LPUPDATE - 对应 analyze line 1531+
//                 94 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_PAY_LPCOST/ADD_COUNTER/REMOVE_COUNTER - 对应 analyze
//                 100|101|102 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_ATTACK/BATTLE/DISABLED/DAMAGE_STEP_START/END - 对应 analyze
//                 110|111|112|113|114 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_MISSED_EFFECT/BE_CHAIN_TARGET/CREATE_RELATION/RELEASE_RELATION - 对应 analyze
//                 120|121|122|123 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_TOSS_COIN/DICE/ROCK_PAPER_SCISSORS/HAND_RES - 对应 analyze
//                 130|131|132|133 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_ANNOUNCE_RACE/ATTRIB/CARD/NUMBER - 对应 analyze
//                 140|141|142|143 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_CARD_HINT/TAG_SWAP/RELOAD_FIELD/AI_NAME/SHOW_HINT/PLAYER_HINT - 对应 analyze
//                 160|161|162|163|164|165 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_MATCH_KILL - 对应 analyze
//                 170 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 // MSG_CUSTOM_MSG - 对应 analyze
//                 180 => { ofs = len; let targets = self.all_targets(); for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf[start..ofs]); } }
//                 _ => { ofs = len; }
//             }
//         }
//         0
//     }

//     fn wait_for_response(&mut self, player: u8, srv: &mut YgoproServer) {
//         self.last_response = player;
//         if let Some(p) = self.players[player as usize] { srv.get_client_mut(p).map(|c| c.state = u8::from(CtosMsg::Response)); }
//     }

//     // ============== Surrender ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:718-743
//     fn surrender_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype > 1 || self.pduel.is_none() { return; }
//         let player = ptype as u32;
//         let wbuf = [u8::from(GameMessage::Win), (1 - player) as u8, 0u8];
//         let targets = self.all_targets();
//         for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &wbuf); }
//         if self.players[player as usize] == self.pplayers[player as usize] { self.match_result[self.duel_count as usize] = (1 - player) as u8; self.tp_player = player as u8; }
//         else { self.match_result[self.duel_count as usize] = player as u8; self.tp_player = (1 - player) as u8; }
//         self.duel_count += 1; self.end_duel_impl(srv); self.duel_end_proc(srv);
//     }

//     fn end_duel_impl(&mut self, _: &mut YgoproServer) { if let Some(pduel) = self.pduel { unsafe { core::end_duel(pduel); } self.pduel = None; } }

//     /// 对应: ../ygopro/gframe/single_duel.cpp:660-717
//     fn duel_end_proc(&mut self, srv: &mut YgoproServer) {
//         if !self.match_mode {
//             for &p in self.players.iter().flatten() { send_stoc_packet(srv, p, u8::from(StocMsg::DuelEnd)); }
//             let obs: Vec<usize> = self.observers.iter().copied().collect();
//             for &o in &obs { send_stoc_packet(srv, o, u8::from(StocMsg::DuelEnd)); }
//             self.duel_stage = u8::from(Duelstage::End);
//         }
//     }

//     // ============== LeaveGame ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:175-274
//     fn leave_game_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype == 7 { self.observers.remove(&client_id); }
//         else if ptype <= 1 && self.duel_stage <= u8::from(Duelstage::Firstgo) {
//             self.players[ptype as usize] = None; self.ready[ptype as usize] = false;
//             let pc = PlayerChange::Leave(Netplayer::try_from(ptype).unwrap_or(Netplayer::Player1));
//             let msg = stoc::HsPlayerChange { status: pc };
//             if let Some(p) = self.players[1 - ptype as usize] { send_stoc_msg(srv, p, &msg, u8::from(StocMsg::HsPlayerChange)); }
//             let obs: Vec<usize> = self.observers.iter().copied().collect();
//             for &o in &obs { send_stoc_msg(srv, o, &msg, u8::from(StocMsg::HsPlayerChange)); }
//         }
//     }

//     // ============== JoinGame ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:34-174
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
//         if self.players[0].is_none() { self.set_player_slot(client_id, 0, srv); }
//         else if self.players[1].is_none() { self.set_player_slot(client_id, 1, srv); }
//         else { self.observers.insert(client_id); srv.get_client_mut(client_id).map(|c| c.client_type = u8::from(Netplayer::Observer)); }
//         for pos in 0..2usize {
//             if let Some(pid) = self.players[pos] { if pid != client_id {
//                 let name = srv.get_client(pid).map(|c| c.name).unwrap_or([0; 20]);
//                 let mut buf = to_bytes_vec(&name); buf.push(pos as u8);
//                 send_stoc_data(srv, client_id, u8::from(StocMsg::HsPlayerEnter), &buf);
//             }}
//         }
//     }

//     fn set_player_slot(&mut self, client_id: usize, pos: u8, srv: &mut YgoproServer) {
//         self.players[pos as usize] = Some(client_id);
//         srv.get_client_mut(client_id).map(|c| c.client_type = pos);
//         let name = srv.get_client(client_id).map(|c| c.name).unwrap_or([0; 20]);
//         let mut buf = to_bytes_vec(&name); buf.push(pos);
//         for &p in self.players.iter().flatten() { send_stoc_data(srv, p, u8::from(StocMsg::HsPlayerEnter), &buf); }
//         let obs: Vec<usize> = self.observers.iter().copied().collect();
//         for &o in &obs { send_stoc_data(srv, o, u8::from(StocMsg::HsPlayerEnter), &buf); }
//     }

//     // ============== ToDuelist ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:275-317
//     fn to_duelist_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype != u8::from(Netplayer::Observer) { return; }
//         if self.players[0].is_some() && self.players[1].is_some() { return; }
//         self.observers.remove(&client_id);
//         let pos: u8 = if self.players[0].is_none() { 0 } else { 1 };
//         self.players[pos as usize] = Some(client_id);
//         srv.get_client_mut(client_id).map(|c| c.client_type = pos);
//         let name = srv.get_client(client_id).map(|c| c.name).unwrap_or([0; 20]);
//         let mut buf = to_bytes_vec(&name); buf.push(pos);
//         for &p in self.players.iter().flatten() { send_stoc_data(srv, p, u8::from(StocMsg::HsPlayerEnter), &buf); }
//         let obs: Vec<usize> = self.observers.iter().copied().collect();
//         for &o in &obs { send_stoc_data(srv, o, u8::from(StocMsg::HsPlayerEnter), &buf); }
//     }

//     // ============== ToObserver ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:318-342
//     fn to_observer_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         let ptype = srv.get_client(client_id).map(|c| c.client_type).unwrap_or(0xff);
//         if ptype > 1 { return; }
//         let pc = PlayerChange::Observe(Netplayer::try_from(ptype).unwrap_or(Netplayer::Player1));
//         let msg = stoc::HsPlayerChange { status: pc };
//         for &p in self.players.iter().flatten() { send_stoc_msg(srv, p, &msg, u8::from(StocMsg::HsPlayerChange)); }
//         let obs: Vec<usize> = self.observers.iter().copied().collect();
//         for &o in &obs { send_stoc_msg(srv, o, &msg, u8::from(StocMsg::HsPlayerChange)); }
//         self.players[ptype as usize] = None; self.ready[ptype as usize] = false;
//         self.observers.insert(client_id);
//         srv.get_client_mut(client_id).map(|c| c.client_type = u8::from(Netplayer::Observer));
//     }

//     // ============== PlayerKick ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp:383-387
//     fn player_kick_impl(&mut self, _: usize, pos: u8, srv: &mut YgoproServer) {
//         if pos > 1 { return; }
//         if let Some(kicked) = self.players[pos as usize] { self.leave_game_impl(kicked, srv); }
//     }

//     // ============== TimeConfirm ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp (TimeConfirm)
//     fn time_confirm_impl(&mut self, _: usize, _: &mut YgoproServer) {
//         self.time_backed[self.last_response as usize] = self.host.time_limit as i16;
//     }

//     // ============== RequestField ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp (RequestField, server only)
//     fn request_field_impl(&mut self, client_id: usize, srv: &mut YgoproServer) {
//         if let Some(pduel) = self.pduel {
//             let mut buf = vec![0u8; SIZE_QUERY_BUFFER_SERVER];
//             let len = unsafe { core::query_field_info(pduel, buf.as_mut_ptr()) };
//             if len > 0 { buf.truncate(len as usize); send_stoc_data(srv, client_id, u8::from(StocMsg::FieldFinish), &buf); }
//         }
//     }

//     // ============== Refresh ==============
//     fn refresh_mzone(&mut self, player: u32, srv: &mut YgoproServer) { self.refresh_field_card(player, Location::MZone.bits(), 0x881fff, srv); }
//     fn refresh_szone(&mut self, player: u32, srv: &mut YgoproServer) { self.refresh_field_card(player, Location::SZone.bits(), 0x681fff, srv); }
//     fn refresh_hand(&mut self, player: u32, srv: &mut YgoproServer) { self.refresh_field_card(player, Location::Hand.bits(), 0x681fff, srv); }
//     fn refresh_extra(&mut self, player: u32, srv: &mut YgoproServer) { self.refresh_field_card(player, Location::Extra.bits(), 0xe81fff, srv); }
//     fn refresh_grave(&mut self, player: u32, srv: &mut YgoproServer) { self.refresh_field_card(player, Location::Grave.bits(), 0x81fff, srv); }
//     fn refresh_removed(&mut self, player: u32, srv: &mut YgoproServer) { self.refresh_field_card(player, Location::Removed.bits(), 0x81fff, srv); }

//     fn refresh_all(&mut self, srv: &mut YgoproServer) {
//         self.refresh_mzone(0, srv); self.refresh_mzone(1, srv);
//         self.refresh_szone(0, srv); self.refresh_szone(1, srv);
//         self.refresh_hand(0, srv); self.refresh_hand(1, srv);
//     }

//     fn refresh_field_card(&mut self, player: u32, location: u8, flag: u32, srv: &mut YgoproServer) {
//         if self.pduel.is_none() { return; }
//         let pduel = self.pduel.unwrap();
//         let mut buf = vec![0u8; SIZE_QUERY_BUFFER_SERVER];
//         let mut ofs = 0usize;
//         write_buffer_u8(&mut buf, &mut ofs, u8::from(GameMessage::UpdateData));
//         write_buffer_u8(&mut buf, &mut ofs, player as u8);
//         write_buffer_u8(&mut buf, &mut ofs, location);
//         let len = unsafe { core::query_field_card(pduel, player as u8, location, flag, buf.as_mut_ptr().add(3) as *mut u8, 1) };
//         if len > 0 {
//             buf.truncate(3 + len as usize);
//             let targets = self.all_targets();
//             for t in targets { send_stoc_data(srv, t, u8::from(StocMsg::GameMessage), &buf); }
//         }
//     }

//     // ============== Tick ==============
//     /// 对应: ../ygopro/gframe/single_duel.cpp (SingleTimer)
//     fn tick_impl(&mut self) -> Vec<OutgoingMessage> {
//         if self.pduel.is_none() { return Vec::new(); }
//         if self.duel_stage != u8::from(Duelstage::Dueling) { return Vec::new(); }
//         if self.host.time_limit == 0 { return Vec::new(); }

//         self.time_elapsed += 1;
//         let player = self.last_response as usize;
//         if self.time_limit[player] <= 0 { return Vec::new(); }
//         self.time_limit[player] -= 1;

//         let tl = stoc::TimeLimit { player: Netplayer::try_from(player as u8).unwrap_or(Netplayer::Player1), left_time: self.time_limit[player] as u16 };
//         self.player_targets().into_iter().map(|t| OutgoingMessage { targets: OutgoingTarget::Single(t), data: to_stoc_bytes(u8::from(StocMsg::TimeLimit), &tl) }).collect()
//     }

//     extern "C" fn message_handler_raw(_: core::intptr_t, _: u32) -> u32 { 0 }
// }

// // ============== DuelMode trait impl ==============
// impl DuelMode for SingleDuel {
//     fn host_info(&self) -> HostInfo { self.host }
//     fn host_info_mut(&mut self) -> &mut HostInfo { &mut self.host }
//     fn pduel(&self) -> Option<core::intptr_t> { self.pduel }
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

// fn to_stoc_bytes<T: BinWrite>(proto: u8, msg: &T) -> Vec<u8> where for<'a> <T as BinWrite>::Args<'a>: Default {
//     let mut buf = Cursor::new(Vec::new()); msg.write_le(&mut buf).ok();
//     let data = buf.into_inner(); let len = (1 + data.len()).min(MAX_DATA_SIZE);
//     let mut result = vec![]; result.extend_from_slice(&(len as u16).to_le_bytes()); result.push(proto); result.extend_from_slice(&data); result
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

// fn build_start_msg(pduel: core::intptr_t, duel_rule: u8, start_lp: i32) -> Vec<u8> {
//     let mut buf = vec![0u8; 32]; let mut ofs = 0usize;
//     write_buffer_u8(&mut buf, &mut ofs, u8::from(GameMessage::Start));
//     write_buffer_u8(&mut buf, &mut ofs, 0);
//     write_buffer_u8(&mut buf, &mut ofs, duel_rule);
//     write_buffer_i32(&mut buf, &mut ofs, start_lp);
//     write_buffer_i32(&mut buf, &mut ofs, start_lp);
//     write_buffer_u16(&mut buf, &mut ofs, unsafe { core::query_field_count(pduel, 0, Location::Deck.bits()) as u16 });
//     write_buffer_u16(&mut buf, &mut ofs, unsafe { core::query_field_count(pduel, 0, Location::Extra.bits()) as u16 });
//     write_buffer_u16(&mut buf, &mut ofs, unsafe { core::query_field_count(pduel, 1, Location::Deck.bits()) as u16 });
//     write_buffer_u16(&mut buf, &mut ofs, unsafe { core::query_field_count(pduel, 1, Location::Extra.bits()) as u16 });
//     buf.truncate(ofs); buf
// }
