// 参考原文: ../ygopro/gframe/single_duel.cpp (完整文件)
// YGOPRO_SERVER_MODE 分支

#include "config.h"
#include "single_duel.h"
#include "netserver.h"
#include "game.h"
#include "data_manager.h"
#include "../ocgcore/mtrandom.h"

namespace ygo {

#ifdef YGOPRO_SERVER_MODE
extern unsigned short replay_mode;
#endif

// ============================================================
// SingleDuel — 单人对局模式，管理玩家加入/离开/准备/换备牌/决斗流程
// ============================================================

// ============================================================
// 构造/析构
// ============================================================

// 构造函数 — 仅记录是否为 match（三局两胜）模式
SingleDuel::SingleDuel(bool is_match) {
	match_mode = is_match;
}
SingleDuel::~SingleDuel() {
}

// ============================================================
// Chat — 聊天消息广播
// dp    : 发消息的玩家
// pdata : 消息原始字节
// len   : 消息字节长度
// ============================================================
// 1. 组装 STOC_CHAT 包，附带发送者身份类型 (dp->type)
// 2. 发给玩家0 → Resend(玩家1) → 遍历观察者 → Resend(录制者)
void SingleDuel::Chat(DuelPlayer* dp, unsigned char* pdata, int len) {
	unsigned char scc[SIZE_STOC_CHAT];
	const auto scc_size = NetServer::CreateChatPacket(pdata, len, scc, dp->type);
	if (!scc_size)
		return;
	NetServer::SendBufferToPlayer(players[0], STOC_CHAT, scc, scc_size);
	NetServer::ReSendToPlayer(players[1]);
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::ReSendToPlayer(*pit);
#ifdef YGOPRO_SERVER_MODE
	// 缓存录制器始终收到
	if(cache_recorder)
		NetServer::ReSendToPlayer(cache_recorder);
	// 云端录制器仅当 REPLAY_MODE_INCLUDE_CHAT 时收到
	if(replay_recorder && replay_mode & REPLAY_MODE_INCLUDE_CHAT)
		NetServer::ReSendToPlayer(replay_recorder);
#endif
}

// ============================================================
// JoinGame — 玩家加入对局（创建者为 is_creater=true，加入者为 false）
// dp    : 发起请求的玩家连接
// pdata : CTOS_JoinGame 原始字节 (创建者传 0，无数据)
// is_creater : 是否为创建房间者
// ============================================================
void SingleDuel::JoinGame(DuelPlayer* dp, unsigned char* pdata, bool is_creater) {

// ===================== 第 1 段 · 录制者标记 =====================
// 服务端模式下用特殊密码识别后台录制连接。
// is_recorder 标记该连接不占用玩家槽位，只是旁听录制。
#ifdef YGOPRO_SERVER_MODE
	bool is_recorder = false;
#endif

// ===================== 第 2 段 · 加入者校验 (非创建者) =====================
// 仅加入者进入此块，创建者跳过校验。
	if(!is_creater) {

// 2a. 防重入 — 已在一个房间内且类型不是 0xff(未初始化) → 拒绝
		if(dp->game && dp->type != 0xff) {
			STOC_ErrorMsg scem;
			scem.msg = ERRMSG_JOINERROR;
			scem.code = 0;
			NetServer::SendPacketToPlayer(dp, STOC_ERROR_MSG, scem);
			NetServer::DisconnectPlayer(dp);
			return;
		}

// 2b. 版本校验 — 客户端版本必须与 PRO_VERSION 一致
		CTOS_JoinGame packet;
		std::memcpy(&packet, pdata, sizeof packet);
		auto pkt = &packet;
		if(pkt->version != PRO_VERSION) {
			STOC_ErrorMsg scem;
			scem.msg = ERRMSG_VERERROR;
			scem.code = PRO_VERSION;          // 带回服务端版本号，供客户端提示更新
			NetServer::SendPacketToPlayer(dp, STOC_ERROR_MSG, scem);
			NetServer::DisconnectPlayer(dp);
			return;
		}

// 2c. 密码检查 — 服务端模式不验证房间密码，而是检测两个硬编码的特殊密码
		wchar_t jpass[20];
		BufferIO::NullTerminate(pkt->pass);
		BufferIO::CopyCharArray(pkt->pass, jpass);

#ifdef YGOPRO_SERVER_MODE
		// "the Big Brother" → 缓存录制器 (Web 回放)
		if(!std::wcscmp(jpass, L"the Big Brother") && !cache_recorder) {
			is_recorder = true;
			cache_recorder = dp;
		}
#ifndef YGOPRO_SERVER_MODE_DISABLE_CLOUD_REPLAY
		// "Marshtomp" → 云端录像录制器
		if(!std::wcscmp(jpass, L"Marshtomp") && !replay_recorder) {
			is_recorder = true;
			replay_recorder = dp;
		}
#endif //YGOPRO_SERVER_MODE_DISABLE_CLOUD_REPLAY

#else
// 2d. 非服务端模式 — 检查密码是否匹配房间 pass
		if(std::wcscmp(jpass, pass)) {
			STOC_ErrorMsg scem;
			scem.msg = ERRMSG_JOINERROR;
			scem.code = 1;
			NetServer::SendPacketToPlayer(dp, STOC_ERROR_MSG, scem);
			return;
		}
#endif //YGOPRO_SERVER_MODE
	}

// ===================== 第 3 段 · 归属 & Host 设定 =====================
// 将该连接关联到当前房间。
// 如果两玩家位全空且无观察者，说明这是第一个进来的人 → 设为房主。
	dp->game = this;
	if(!players[0] && !players[1] && observers.size() == 0)
		host_player = dp;

// ===================== 第 4 段 · 构造基础回包 =====================
// scjg: 告知客户端房间规则配置 (禁卡表、起始LP、手牌数、时限等)
// sctc: 告知客户端自己的身份类型，0x10 位表示"你是房主"
	STOC_JoinGame scjg;
	scjg.info = host_info;
	STOC_TypeChange sctc;
	sctc.type = (host_player == dp) ? 0x10 : 0;

// ===================== 第 5 段 · 身份分配 =====================
// 根据当前房间容量，决定该连接的角色：录制者 / 玩家 / 观察者

#ifdef YGOPRO_SERVER_MODE
// 5a. 录制者 — 静默加入，不通知任何人
	if(is_recorder) {
		dp->type = 9;
		sctc.type = NETPLAYER_TYPE_OBSERVER;
	}
	else
#endif

// 5b. 有空位 → 成为玩家
	if(!players[0] || !players[1]) {

		// 5b-1. 构造 PlayerEnter 包 (新玩家的名字 + 位置)
		STOC_HS_PlayerEnter scpe;
		BufferIO::CopyCharArray(dp->name, scpe.name);
		if(!players[0])
			scpe.pos = 0;
		else
			scpe.pos = 1;

		// 5b-2. 向已在场的所有人广播 "有人进来了"
		if(players[0]) {
			NetServer::SendPacketToPlayer(players[0], STOC_HS_PLAYER_ENTER, scpe);
		}
		if(players[1]) {
			NetServer::SendPacketToPlayer(players[1], STOC_HS_PLAYER_ENTER, scpe);
		}
		for(auto pit = observers.begin(); pit != observers.end(); ++pit)
			NetServer::SendPacketToPlayer(*pit, STOC_HS_PLAYER_ENTER, scpe);
#ifdef YGOPRO_SERVER_MODE
		if(cache_recorder)
			NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_PLAYER_ENTER, scpe);
		if(replay_recorder)
			NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_PLAYER_ENTER, scpe);
#endif

		// 5b-3. 将 dp 填入空位，设玩家类型，合并到 sctc.type
		if(!players[0]) {
			players[0] = dp;
			dp->type = NETPLAYER_TYPE_PLAYER1;
			sctc.type |= NETPLAYER_TYPE_PLAYER1;
		} else {
			players[1] = dp;
			dp->type = NETPLAYER_TYPE_PLAYER2;
			sctc.type |= NETPLAYER_TYPE_PLAYER2;
		}

// 5c. 无空位 → 成为观察者
	} else {
		observers.insert(dp);
		dp->type = NETPLAYER_TYPE_OBSERVER;
		sctc.type |= NETPLAYER_TYPE_OBSERVER;

		// 向所有人广播更新后的观战人数
		STOC_HS_WatchChange scwc;
		scwc.watch_count = (unsigned short)observers.size();
		if(players[0])
			NetServer::SendPacketToPlayer(players[0], STOC_HS_WATCH_CHANGE, scwc);
		if(players[1])
			NetServer::SendPacketToPlayer(players[1], STOC_HS_WATCH_CHANGE, scwc);
		for(auto pit = observers.begin(); pit != observers.end(); ++pit)
			NetServer::SendPacketToPlayer(*pit, STOC_HS_WATCH_CHANGE, scwc);
#ifdef YGOPRO_SERVER_MODE
		if(cache_recorder)
			NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_WATCH_CHANGE, scwc);
		if(replay_recorder)
			NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_WATCH_CHANGE, scwc);
#endif
	}

// ===================== 第 6 段 · 向加入者发送当前房间全貌 =====================
// 经过上述分配后，现在告诉新来的人"这就是你所在的房间"。

// 6a. 基础信息：房间配置 + 你的身份类型
	NetServer::SendPacketToPlayer(dp, STOC_JOIN_GAME, scjg);
	NetServer::SendPacketToPlayer(dp, STOC_TYPE_CHANGE, sctc);

// 6b. 已存在的玩家0 的信息 (名字 + ready 状态)
	if(players[0]) {
		STOC_HS_PlayerEnter scpe;
		BufferIO::CopyCharArray(players[0]->name, scpe.name);
		scpe.pos = 0;
		NetServer::SendPacketToPlayer(dp, STOC_HS_PLAYER_ENTER, scpe);
		if(ready[0]) {
			STOC_HS_PlayerChange scpc;
			scpc.status = PLAYERCHANGE_READY;
			NetServer::SendPacketToPlayer(dp, STOC_HS_PLAYER_CHANGE, scpc);
		}
	}

// 6c. 已存在的玩家1 的信息 — status 高位带 0x10 位移标识玩家1
	if(players[1]) {
		STOC_HS_PlayerEnter scpe;
		BufferIO::CopyCharArray(players[1]->name, scpe.name);
		scpe.pos = 1;
		NetServer::SendPacketToPlayer(dp, STOC_HS_PLAYER_ENTER, scpe);
		if(ready[1]) {
			STOC_HS_PlayerChange scpc;
			scpc.status = 0x10 | PLAYERCHANGE_READY;
			NetServer::SendPacketToPlayer(dp, STOC_HS_PLAYER_CHANGE, scpc);
		}
	}

// 6d. 观察者人数
	if(observers.size()) {
		STOC_HS_WatchChange scwc;
		scwc.watch_count = (unsigned short)observers.size();
		NetServer::SendPacketToPlayer(dp, STOC_HS_WATCH_CHANGE, scwc);
	}
}

// ============================================================
// LeaveGame — 玩家离开对局
// dp : 要离开的玩家
//
// 行为因 dp 身份而异：
//  - 房主离开 → 尝试转移房主到另一玩家，不行则 EndDuel + StopServer
//  - 观察者离开 → 从 observers 移除，广播 WatchChange
//  - 玩家离开 (BEGIN 阶段) → 清除玩家槽位，广播 PlayerChange
//  - 玩家离开 (SIDING 阶段) → 催促另一方开始
//  - 玩家离开 (DUELING 阶段) → 判对方胜利，EndDuel
// ============================================================
void SingleDuel::LeaveGame(DuelPlayer* dp) {

// ===================== 第 1 段 · 房主离开 → 转移房主 =====================
	if(dp == host_player) {

#ifdef YGOPRO_SERVER_MODE
// 1a. 服务端模式 — 选出非自己玩家作为新房主
		int host_pos;
		if(players[0] && dp->type != 0) {
			host_pos = 0;
			host_player = players[0];
		} else if(players[1] && dp->type != 1) {
			host_pos = 1;
			host_player = players[1];
		} else {
			// 无人可接替房主 → 终结对局
			EndDuel();
			NetServer::StopServer();
			return;
		}
		// BEGIN 阶段：重置新房主 ready 状态 + 更新其 TypeChange
		if(duel_stage == DUEL_STAGE_BEGIN) {
			ready[host_pos] = false;
			STOC_TypeChange sctc;
			sctc.type = 0x10 | host_pos;
			NetServer::SendPacketToPlayer(players[host_pos], STOC_TYPE_CHANGE, sctc);
		}
	}

// 1b. 观察者离开 (服务端模式下可单独处理)
	if(dp->type == NETPLAYER_TYPE_OBSERVER) {

#else
// 1c. 非服务端模式 — 房主离开直接终结对局
		EndDuel();
		NetServer::StopServer();
	} else if(dp->type == NETPLAYER_TYPE_OBSERVER) {
#endif //YGOPRO_SERVER_MODE

// ===================== 第 2 段 · 观察者离开 =====================
// 从集合移除，BEGIN 阶段广播新人数
		observers.erase(dp);
		if(duel_stage == DUEL_STAGE_BEGIN) {
			STOC_HS_WatchChange scwc;
			scwc.watch_count = (unsigned short)observers.size();
			if(players[0])
				NetServer::SendPacketToPlayer(players[0], STOC_HS_WATCH_CHANGE, scwc);
			if(players[1])
				NetServer::SendPacketToPlayer(players[1], STOC_HS_WATCH_CHANGE, scwc);
			for(auto pit = observers.begin(); pit != observers.end(); ++pit)
				NetServer::SendPacketToPlayer(*pit, STOC_HS_WATCH_CHANGE, scwc);
#ifdef YGOPRO_SERVER_MODE
			if(cache_recorder)
				NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_WATCH_CHANGE, scwc);
			if(replay_recorder)
				NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_WATCH_CHANGE, scwc);
#endif
		}
		NetServer::DisconnectPlayer(dp);

// ===================== 第 3 段 · 玩家离开 =====================
	} else {

// 3a. BEGIN 阶段 — 清空槽位 + 广播 PlayerChange(LEAVE)
		if(duel_stage == DUEL_STAGE_BEGIN) {
			STOC_HS_PlayerChange scpc;
			players[dp->type] = 0;
			ready[dp->type] = false;
			scpc.status = (dp->type << 4) | PLAYERCHANGE_LEAVE;
			if(players[0] && dp->type != 0)
				NetServer::SendPacketToPlayer(players[0], STOC_HS_PLAYER_CHANGE, scpc);
			if(players[1] && dp->type != 1)
				NetServer::SendPacketToPlayer(players[1], STOC_HS_PLAYER_CHANGE, scpc);
			for(auto pit = observers.begin(); pit != observers.end(); ++pit)
				NetServer::SendPacketToPlayer(*pit, STOC_HS_PLAYER_CHANGE, scpc);
#ifdef YGOPRO_SERVER_MODE
			if(cache_recorder)
				NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_PLAYER_CHANGE, scpc);
			if(replay_recorder)
				NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_PLAYER_CHANGE, scpc);
#endif
			NetServer::DisconnectPlayer(dp);

// 3b. SIDING 阶段 — 催促未 ready 方开始 (相当于自动 ready)
		} else {
			if(duel_stage == DUEL_STAGE_SIDING) {
				if(!ready[0])
					NetServer::SendPacketToPlayer(players[0], STOC_DUEL_START);
				if(!ready[1])
					NetServer::SendPacketToPlayer(players[1], STOC_DUEL_START);
			}

// 3c. DUELING 阶段 — 判对方胜利 (reason = 0x4 = 对手离开)
			if(duel_stage != DUEL_STAGE_END) {
				unsigned char wbuf[3];
				wbuf[0] = MSG_WIN;
				wbuf[1] = 1 - dp->type;
				wbuf[2] = 0x4;
				NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, wbuf, 3);
				NetServer::ReSendToPlayer(players[1]);
				for(auto oit = observers.begin(); oit != observers.end(); ++oit)
					NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
				NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
				EndDuel();
				NetServer::SendPacketToPlayer(players[0], STOC_DUEL_END);
				NetServer::ReSendToPlayer(players[1]);
				for(auto oit = observers.begin(); oit != observers.end(); ++oit)
					NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
				NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
				NetServer::StopServer();
#endif
			}
#ifndef YGOPRO_SERVER_MODE
			NetServer::DisconnectPlayer(dp);
#endif
		}
	}
}

// ============================================================
// ToDuelist — 观察者变为玩家（上桌）
// dp : 要上桌的观察者
// 前置条件：dp 必须是观察者，且有空位。
// 从 observers 移除 → 填入空位 → 广播 PlayerEnter + WatchChange → 向 dp 发送 TypeChange
// ============================================================
void SingleDuel::ToDuelist(DuelPlayer* dp) {

// 1. 前置校验 — 不是观察者或没空位则直接返回
	if(dp->type != NETPLAYER_TYPE_OBSERVER)
		return;
	if(players[0] && players[1])
		return;

// 2. 从观察者集合移除，填入空位
	observers.erase(dp);
	STOC_HS_PlayerEnter scpe;
	BufferIO::CopyCharArray(dp->name, scpe.name);
	if(!players[0]) {
		players[0] = dp;
		dp->type = NETPLAYER_TYPE_PLAYER1;
		scpe.pos = 0;
	} else {
		players[1] = dp;
		dp->type = NETPLAYER_TYPE_PLAYER2;
		scpe.pos = 1;
	}

// 3. 广播 PlayerEnter + WatchChange 给所有人
	STOC_HS_WatchChange scwc;
	scwc.watch_count = (unsigned short)observers.size();
	NetServer::SendPacketToPlayer(players[0], STOC_HS_PLAYER_ENTER, scpe);
	NetServer::SendPacketToPlayer(players[0], STOC_HS_WATCH_CHANGE, scwc);
	if(players[1]) {
		NetServer::SendPacketToPlayer(players[1], STOC_HS_PLAYER_ENTER, scpe);
		NetServer::SendPacketToPlayer(players[1], STOC_HS_WATCH_CHANGE, scwc);
	}
	for(auto pit = observers.begin(); pit != observers.end(); ++pit) {
		NetServer::SendPacketToPlayer(*pit, STOC_HS_PLAYER_ENTER, scpe);
		NetServer::SendPacketToPlayer(*pit, STOC_HS_WATCH_CHANGE, scwc);
	}
#ifdef YGOPRO_SERVER_MODE
	if(cache_recorder) {
		NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_PLAYER_ENTER, scpe);
		NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_WATCH_CHANGE, scwc);
	}
	if(replay_recorder) {
		NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_PLAYER_ENTER, scpe);
		NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_WATCH_CHANGE, scwc);
	}
#endif

// 4. 通知 dp 自己的新身份
	STOC_TypeChange sctc;
	sctc.type = (dp == host_player ? 0x10 : 0) | dp->type;
	NetServer::SendPacketToPlayer(dp, STOC_TYPE_CHANGE, sctc);
}

// ============================================================
// ToObserver — 玩家变为观察者（下桌）
// dp : 要下桌的玩家，必须是玩家 (type 0 或 1)。
// 清空玩家槽位 → 广播 PlayerChange(OBSERVE) → 加入 observers → 向 dp 发送 TypeChange
// ============================================================
void SingleDuel::ToObserver(DuelPlayer* dp) {

// 1. 只允许玩家 (type 0/1) 变观察者
	if(dp->type > 1)
		return;

// 2. 广播 PlayerChange(OBSERVE)
	STOC_HS_PlayerChange scpc;
	scpc.status = (dp->type << 4) | PLAYERCHANGE_OBSERVE;
	if(players[0])
		NetServer::SendPacketToPlayer(players[0], STOC_HS_PLAYER_CHANGE, scpc);
	if(players[1])
		NetServer::SendPacketToPlayer(players[1], STOC_HS_PLAYER_CHANGE, scpc);
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::SendPacketToPlayer(*pit, STOC_HS_PLAYER_CHANGE, scpc);
#ifdef YGOPRO_SERVER_MODE
	if(cache_recorder)
		NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_PLAYER_CHANGE, scpc);
	if(replay_recorder)
		NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_PLAYER_CHANGE, scpc);
#endif

// 3. 清空槽位，重置 ready，加入 observers
	players[dp->type] = 0;
	ready[dp->type] = false;
	dp->type = NETPLAYER_TYPE_OBSERVER;
	observers.insert(dp);

// 4. 通知 dp 自己的新身份
	STOC_TypeChange sctc;
	sctc.type = (dp == host_player ? 0x10 : 0) | dp->type;
	NetServer::SendPacketToPlayer(dp, STOC_TYPE_CHANGE, sctc);
}

// ============================================================
// PlayerReady — 玩家准备/取消准备
// dp       : 操作的玩家
// is_ready : true = 准备, false = 取消
//
// 准备时（is_ready=true）：
//   1. 如果不禁用卡组检查 (no_check_deck=false)，则验证卡组合法性
//   2. 卡组有误 → 发送 NOTREADY + ERRMSG_DECKERROR，不设 ready
//   3. 卡组无误 → 设 ready=true，广播
// ============================================================
void SingleDuel::PlayerReady(DuelPlayer* dp, bool is_ready) {

// 1. 仅玩家 (type 0/1) 可准备；状态无变化则跳过
	if(dp->type > 1)
		return;
	if(ready[dp->type] == is_ready)
		return;

// 2. 准备时的卡组校验
	if(is_ready) {
		uint32_t deckerror = 0;

// 2a. 如果不禁用卡组检查，则两步校验：先看 deck_error 缓存，再做正式 CheckDeck
		if(!host_info.no_check_deck) {
			if(deck_error[dp->type]) {
				deckerror = (DECKERROR_UNKNOWNCARD << 28) | deck_error[dp->type];
			} else {
				deckerror = deckManager.CheckDeck(pdeck[dp->type], host_info.lflist, host_info.rule);
			}
		}

// 2b. 卡组有误 → 回退为 NOTREADY，发送 deckerror
		if(deckerror) {
			STOC_HS_PlayerChange scpc;
			scpc.status = (dp->type << 4) | PLAYERCHANGE_NOTREADY;
			NetServer::SendPacketToPlayer(dp, STOC_HS_PLAYER_CHANGE, scpc);
			STOC_ErrorMsg scem;
			scem.msg = ERRMSG_DECKERROR;
			scem.code = deckerror;
			NetServer::SendPacketToPlayer(dp, STOC_ERROR_MSG, scem);
			return;
		}
	}

// 3. 更新 ready 状态，广播 PlayerChange 给所有人
	ready[dp->type] = is_ready;
	STOC_HS_PlayerChange scpc;
	scpc.status = (dp->type << 4) | (is_ready ? PLAYERCHANGE_READY : PLAYERCHANGE_NOTREADY);
	NetServer::SendPacketToPlayer(players[dp->type], STOC_HS_PLAYER_CHANGE, scpc);
	if(players[1 - dp->type])
		NetServer::SendPacketToPlayer(players[1 - dp->type], STOC_HS_PLAYER_CHANGE, scpc);
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::SendPacketToPlayer(*pit, STOC_HS_PLAYER_CHANGE, scpc);
#ifdef YGOPRO_SERVER_MODE
	if(cache_recorder)
		NetServer::SendPacketToPlayer(cache_recorder, STOC_HS_PLAYER_CHANGE, scpc);
	if(replay_recorder)
		NetServer::SendPacketToPlayer(replay_recorder, STOC_HS_PLAYER_CHANGE, scpc);
#endif
}

// ============================================================
// PlayerKick — 房主踢人
// dp  : 操作的房主
// pos : 要踢的玩家槽位 (0 或 1)
// 前提：pos 合法，dp 是房主，dp != players[pos]，该槽位有人
// 效果：调用 LeaveGame 让被踢玩家离开
// ============================================================
void SingleDuel::PlayerKick(DuelPlayer* dp, unsigned char pos) {
	if(pos > 1 || dp != host_player || dp == players[pos] || !players[pos])
		return;
	LeaveGame(players[pos]);
}

// ============================================================
// UpdateDeck — 玩家提交/更新卡组
// dp    : 提交卡组的玩家
// pdata : 卡组字节 (mainc + sidec + 卡号列表)
// len   : 字节长度
//
// 两个阶段：
//   duel_count == 0 (首局) → LoadDeck 后自动 PlayerReady
//   duel_count != 0 (SIDING) → LoadSide，双方都换好后进入 FIRSTGO 选先后攻
// ============================================================
void SingleDuel::UpdateDeck(DuelPlayer* dp, unsigned char* pdata, unsigned int len) {

// 1. 前置校验 — 仅玩家可提交，已 ready 不可提交
	if(dp->type > 1 || ready[dp->type])
		return;

// 2. 长度校验 — 至少需要 mainc(4) + sidec(4)
	if (len < sizeof(uint32_t) * 2)
		return;

// 3. 数量合法性校验
	bool valid = true;
	uint32_t mainc = BufferIO::Read<uint32_t>(pdata);
	uint32_t sidec = BufferIO::Read<uint32_t>(pdata);
	if (mainc > MAINC_MAX)
		valid = false;
	else if (sidec > SIDEC_MAX)
		valid = false;
	else if (len < (2 + mainc + sidec) * sizeof(uint32_t))
		valid = false;

// 4. 无效卡组 → 报错
	if (!valid) {
#ifdef YGOPRO_SERVER_MODE
		if(duel_count == 0) {
			STOC_HS_PlayerChange scpc;
			scpc.status = (dp->type << 4) | PLAYERCHANGE_NOTREADY;
			NetServer::SendPacketToPlayer(dp, STOC_HS_PLAYER_CHANGE, scpc);
		}
#endif
		STOC_ErrorMsg scem;
		scem.msg = ERRMSG_DECKERROR;
		scem.code = 0;
		NetServer::SendPacketToPlayer(dp, STOC_ERROR_MSG, scem);
		return;
	}

// 5. 拷贝卡组数据
	uint32_t deckbuf[MAINC_MAX + SIDEC_MAX];
	std::memcpy(deckbuf, pdata, (mainc + sidec) * sizeof(uint32_t));

	if(duel_count == 0) {

// 5a. 首局 → LoadDeck，记录 deck_error（供后续 PlayerReady 使用）
		deck_error[dp->type] = DeckManager::LoadDeck(pdeck[dp->type], deckbuf, mainc, sidec);
#ifdef YGOPRO_SERVER_MODE
		// 服务端模式自动 ready
		PlayerReady(dp, true);
#endif
	} else {

// 5b. SIDING 阶段 → LoadSide，双方都换好后进 FIRSTGO
		if(DeckManager::LoadSide(pdeck[dp->type], deckbuf, mainc, sidec)) {
			ready[dp->type] = true;
			NetServer::SendPacketToPlayer(dp, STOC_DUEL_START);
			if(ready[0] && ready[1]) {
				// 双方 ready → 让 tp_player 选先后攻
				NetServer::SendPacketToPlayer(players[tp_player], STOC_SELECT_TP);
				players[1 - tp_player]->state = 0xff;
				players[tp_player]->state = CTOS_TP_RESULT;
				duel_stage = DUEL_STAGE_FIRSTGO;
			}
		} else {
			// 换备牌无效
			STOC_ErrorMsg scem;
			scem.msg = ERRMSG_SIDEERROR;
			scem.code = 0;
			NetServer::SendPacketToPlayer(dp, STOC_ERROR_MSG, scem);
		}
	}
}

// ============================================================
// StartDuel — 房主开始决斗
// dp : 操作的玩家（必须是房主）
//
// 流程：
//   1. 双方 ready 后开始
//   2. StopListen 停止接受新连接
//   3. 发 DuelStart + 观察者 state 设为 LeaveGame
//   4. 交换 deck_count 信息（双方视角交换）
//   5. SelectHand（猜拳），state → HAND_RESULT
// ============================================================
void SingleDuel::StartDuel(DuelPlayer* dp) {

// 1. 仅房主可开始，双方必须 ready
	if(dp != host_player)
		return;
	if(!ready[0] || !ready[1])
		return;

// 2. 停服，不再接收新连接
	NetServer::StopListen();

// 3. 发 DuelStart 给双方，观察者 state 置为 LeaveGame
	NetServer::SendPacketToPlayer(players[0], STOC_DUEL_START);
	NetServer::ReSendToPlayer(players[1]);
	for(auto oit = observers.begin(); oit != observers.end(); ++oit) {
		(*oit)->state = CTOS_LEAVE_GAME;
		NetServer::ReSendToPlayer(*oit);
	}
#ifdef YGOPRO_SERVER_MODE
	if(cache_recorder)
		cache_recorder->state = CTOS_LEAVE_GAME;
	if(replay_recorder)
		replay_recorder->state = CTOS_LEAVE_GAME;
	NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif

// 4. 交换卡组计数 — 双方视角相反（玩家0看到的是玩家1的卡组信息，反之亦然）
	unsigned char deckbuff[12];
	auto pbuf = deckbuff;
	BufferIO::Write<uint16_t>(pbuf, (uint16_t)pdeck[0].main.size());
	BufferIO::Write<uint16_t>(pbuf, (uint16_t)pdeck[0].extra.size());
	BufferIO::Write<uint16_t>(pbuf, (uint16_t)pdeck[0].side.size());
	BufferIO::Write<uint16_t>(pbuf, (uint16_t)pdeck[1].main.size());
	BufferIO::Write<uint16_t>(pbuf, (uint16_t)pdeck[1].extra.size());
	BufferIO::Write<uint16_t>(pbuf, (uint16_t)pdeck[1].side.size());
	NetServer::SendBufferToPlayer(players[0], STOC_DECK_COUNT, deckbuff, 12);
	// 玩家1需要交换前后各6字节
	char tempbuff[6];
	std::memcpy(tempbuff, deckbuff, 6);
	std::memcpy(deckbuff, deckbuff + 6, 6);
	std::memcpy(deckbuff + 6, tempbuff, 6);
	NetServer::SendBufferToPlayer(players[1], STOC_DECK_COUNT, deckbuff, 12);

// 5. 猜拳 — 双方进入 HAND_RESULT 状态
	NetServer::SendPacketToPlayer(players[0], STOC_SELECT_HAND);
	NetServer::ReSendToPlayer(players[1]);
	hand_result[0] = 0;
	hand_result[1] = 0;
	players[0]->state = CTOS_HAND_RESULT;
	players[1]->state = CTOS_HAND_RESULT;
	duel_stage = DUEL_STAGE_FINGER;
}

// ============================================================
// HandResult — 玩家猜拳结果
// dp  : 提交结果的玩家
// res : 猜拳结果 (1=石头, 2=剪刀, 3=布)
//
// 双方都出结果后：
//   平局 → 重新猜拳
//   赢家 → 选先后攻 (SELECT_TP)
// ============================================================
void SingleDuel::HandResult(DuelPlayer* dp, unsigned char res) {

// 1. 有效性校验
	if(res > 3)
		return;
	if(dp->state != CTOS_HAND_RESULT)
		return;

// 2. 记录结果
	hand_result[dp->type] = res;

// 3. 双方都出结果后判断
	if(hand_result[0] && hand_result[1]) {

// 3a. 发送结果给双方（视角互换）
		STOC_HandResult schr;
		schr.res1 = hand_result[0];
		schr.res2 = hand_result[1];
		NetServer::SendPacketToPlayer(players[0], STOC_HAND_RESULT, schr);
		for(auto oit = observers.begin(); oit != observers.end(); ++oit)
			NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
		NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
		schr.res1 = hand_result[1];
		schr.res2 = hand_result[0];
		NetServer::SendPacketToPlayer(players[1], STOC_HAND_RESULT, schr);

// 3b. 判断输赢 — 剪刀石头布的胜负环
		if(hand_result[0] == hand_result[1]) {
			// 平局 → 重新猜拳
			NetServer::SendPacketToPlayer(players[0], STOC_SELECT_HAND);
			NetServer::ReSendToPlayer(players[1]);
			hand_result[0] = 0;
			hand_result[1] = 0;
			players[0]->state = CTOS_HAND_RESULT;
			players[1]->state = CTOS_HAND_RESULT;
		} else if((hand_result[0] == 1 && hand_result[1] == 2)
		          || (hand_result[0] == 2 && hand_result[1] == 3)
		          || (hand_result[0] == 3 && hand_result[1] == 1)) {
			// 玩家0 赢 → 玩家1 选先后攻
			NetServer::SendPacketToPlayer(players[1], STOC_SELECT_TP);
			tp_player = 1;
			players[0]->state = 0xff;
			players[1]->state = CTOS_TP_RESULT;
			duel_stage = DUEL_STAGE_FIRSTGO;
		} else {
			// 玩家1 赢 → 玩家0 选先后攻
			NetServer::SendPacketToPlayer(players[0], STOC_SELECT_TP);
			players[1]->state = 0xff;
			players[0]->state = CTOS_TP_RESULT;
			tp_player = 0;
			duel_stage = DUEL_STAGE_FIRSTGO;
		}
	}
}

// ============================================================
// TPResult — 玩家选择先后攻
// dp : 选先后攻的玩家
// tp : 选择 (0=先攻, 1=后攻)
//
// 如果选了后攻且 dp->type 与选择不匹配，则交换 players[0/1] 和卡组。
// 初始化引擎：随机种子、回放头、卡组、起手 LP/手牌、start_duel。
// ============================================================
void SingleDuel::TPResult(DuelPlayer* dp, unsigned char tp) {

// 1. 状态校验
	if(dp->state != CTOS_TP_RESULT)
		return;

	duel_stage = DUEL_STAGE_DUELING;
	bool swapped = false;

// 2. 保存原始指针 (match 模式结束后恢复用)
	pplayer[0] = players[0];
	pplayer[1] = players[1];

// 3. 交换逻辑：选了后攻时交换玩家槽位
	if((tp && dp->type == 1) || (!tp && dp->type == 0)) {
		std::swap(players[0], players[1]);
		players[0]->type = 0;
		players[1]->type = 1;
		std::swap(pdeck[0], pdeck[1]);
		swapped = true;
	}

// 4. 进入响应状态
	dp->state = CTOS_RESPONSE;

// 5. 生成随机种子 (服务端模式可使用预置种子)
	std::random_device rd;
	ExtendedReplayHeader rh;
	rh.base.id = REPLAY_ID_YRP2;
	rh.base.version = PRO_VERSION;
	rh.base.flag = REPLAY_UNIFORM;
	rh.base.start_time = (uint32_t)std::time(nullptr);
#ifdef YGOPRO_SERVER_MODE
	if (pre_seed_specified[duel_count])
		memcpy(rh.seed_sequence, pre_seed[duel_count], SEED_COUNT * sizeof(uint32_t));
	else
#endif
	for (auto& x : rh.seed_sequence)
		x = rd();

// 6. 开始回放记录
	mtrandom rnd(rh.seed_sequence, SEED_COUNT);
	last_replay.BeginRecord();
	last_replay.WriteHeader(rh);
	last_replay.WriteData(players[0]->name, 40, false);
	last_replay.WriteData(players[1]->name, 40, false);

// 7. 洗牌（除非禁用 no_shuffle_deck）
	if(!host_info.no_shuffle_deck) {
		rnd.shuffle_vector(pdeck[0].main);
		rnd.shuffle_vector(pdeck[1].main);
	}

// 8. 初始化时限
	time_limit[0] = host_info.time_limit;
	time_limit[1] = host_info.time_limit;

// 9. 设置引擎回调 + 创建 duel
	set_script_reader(DataManager::ScriptReaderEx);
	set_card_reader(DataManager::CardReader);
	set_message_handler(SingleDuel::MessageHandler);
	pduel = create_duel_v2(rh.seed_sequence);

// 10. 设置玩家信息（LP、起手、抽卡数）
	set_player_info(pduel, 0, host_info.start_lp, host_info.start_hand, host_info.draw_count);
	set_player_info(pduel, 1, host_info.start_lp, host_info.start_hand, host_info.draw_count);

#ifdef YGOPRO_SERVER_MODE
	preload_script(pduel, "./script/special.lua");
#endif

// 11. 设置决斗选项 (规则 | 伪洗牌)
	unsigned int opt = (unsigned int)host_info.duel_rule << 16;
	if(host_info.no_shuffle_deck)
		opt |= DUEL_PSEUDO_SHUFFLE;

// 12. 回放写入规则信息
	last_replay.WriteInt32(host_info.start_lp, false);
	last_replay.WriteInt32(host_info.start_hand, false);
	last_replay.WriteInt32(host_info.draw_count, false);
	last_replay.WriteInt32(opt, false);
	last_replay.Flush();

// 13. 加载双方卡组到引擎 (倒序插入，所有卡 POS_FACEDOWN_DEFENSE)
	auto load = [&](const std::vector<const CardDataC*>& deck_container, uint8_t p, uint8_t location) {
		last_replay.WriteInt32(deck_container.size(), false);
		for (auto cit = deck_container.rbegin(); cit != deck_container.rend(); ++cit) {
			new_card(pduel, (*cit)->code, p, p, location, 0, POS_FACEDOWN_DEFENSE);
			last_replay.WriteInt32((*cit)->code, false);
		}
	};
	load(pdeck[0].main, 0, LOCATION_DECK);
	load(pdeck[0].extra, 0, LOCATION_EXTRA);
	load(pdeck[1].main, 1, LOCATION_DECK);
	load(pdeck[1].extra, 1, LOCATION_EXTRA);
	last_replay.Flush();

// 14. 构造并发送 MSG_START 消息
	unsigned char startbuf[32]{};
	auto pbuf = startbuf;
	BufferIO::Write<uint8_t>(pbuf, MSG_START);
	BufferIO::Write<uint8_t>(pbuf, 0);
	BufferIO::Write<uint8_t>(pbuf, host_info.duel_rule);
	BufferIO::Write<int32_t>(pbuf, host_info.start_lp);
	BufferIO::Write<int32_t>(pbuf, host_info.start_lp);
	BufferIO::Write<uint16_t>(pbuf, query_field_count(pduel, 0, LOCATION_DECK));
	BufferIO::Write<uint16_t>(pbuf, query_field_count(pduel, 0, LOCATION_EXTRA));
	BufferIO::Write<uint16_t>(pbuf, query_field_count(pduel, 1, LOCATION_DECK));
	BufferIO::Write<uint16_t>(pbuf, query_field_count(pduel, 1, LOCATION_EXTRA));
	NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, startbuf, 19);
	startbuf[1] = 1;
	NetServer::SendBufferToPlayer(players[1], STOC_GAME_MSG, startbuf, 19);
	// 观察者看到 0x10/0x11（取决于是否交换了先后攻）
	if(!swapped)
		startbuf[1] = 0x10;
	else
		startbuf[1] = 0x11;
	for(auto oit = observers.begin(); oit != observers.end(); ++oit)
		NetServer::SendBufferToPlayer(*oit, STOC_GAME_MSG, startbuf, 19);
#ifdef YGOPRO_SERVER_MODE
	if(cache_recorder)
		NetServer::SendBufferToPlayer(cache_recorder, STOC_GAME_MSG, startbuf, 19);
	if(replay_recorder)
		NetServer::SendBufferToPlayer(replay_recorder, STOC_GAME_MSG, startbuf, 19);
	turn_player = 0;
	phase = 1;
	deck_reversed = false;
#endif

// 15. 刷新额外区，启动决斗
	RefreshExtra(0);
	RefreshExtra(1);
	start_duel(pduel, opt);

// 16. 启动计时器 (如果有时间限制)
	if(host_info.time_limit) {
		time_elapsed = 0;
#ifdef YGOPRO_SERVER_MODE
		time_compensator[0] = host_info.time_limit;
		time_compensator[1] = host_info.time_limit;
		time_backed[0] = host_info.time_limit;
		time_backed[1] = host_info.time_limit;
		last_game_msg = 0;
#endif
		timeval timeout = { 1, 0 };
		event_add(etimer, &timeout);
	}

// 17. 开始主循环处理
	Process();
}

// ============================================================
// Process — 决斗主循环
//
// 循环调用 process(pduel) → get_message → Analyze 直至：
//   engFlag == PROCESSOR_END 或 stop != 0
// stop == 2 时调用 DuelEndProc 结束决斗
// ============================================================
void SingleDuel::Process() {
	std::vector<unsigned char> engineBuffer;
	engineBuffer.reserve(SIZE_MESSAGE_BUFFER);
	unsigned int engFlag = 0;
	int engLen = 0;
	int stop = 0;

// 循环处理引擎消息，直到结束或需要等待响应
	while (!stop) {
		if (engFlag == PROCESSOR_END)
			break;

		// 调用引擎 process，低 16 位是缓冲区长度，高 16 位是状态标志
		unsigned int result = process(pduel);
		engLen = result & PROCESSOR_BUFFER_LEN;
		engFlag = result & PROCESSOR_FLAG;

		if (engLen > 0) {
			if (engLen > (int)engineBuffer.size())
				engineBuffer.resize(engLen);
			get_message(pduel, engineBuffer.data());
			// Analyze 返回 0=继续, 1=等待响应, 2=结束
			stop = Analyze(engineBuffer.data(), engLen);
		}
	}
	if(stop == 2)
		DuelEndProc();
}

// ============================================================
// DuelEndProc — 决斗结束处理
//
// 非 match 模式：直接发 DuelEnd + StopServer (服务端模式)
// match 模式：满足结束条件 → DuelEnd + StopServer；未满足 → CHANGE_SIDE 换备牌
// ============================================================
void SingleDuel::DuelEndProc() {
	if(!match_mode) {

// ===================== 非 match 模式：直接结束 =====================
		NetServer::SendPacketToPlayer(players[0], STOC_DUEL_END);
		NetServer::ReSendToPlayer(players[1]);
		for(auto oit = observers.begin(); oit != observers.end(); ++oit)
			NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
		NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
		NetServer::StopServer();
#else
		duel_stage = DUEL_STAGE_END;
#endif

	} else {

// ===================== match 模式：统计胜负 =====================
		int winc[3] = {0, 0, 0};
		for(int i = 0; i < duel_count; ++i)
			winc[match_result[i]]++;

// match 结束条件：match_kill / 某方两胜 / 某方一胜两平 / 三局全平 / 双一胜一平
		if(match_kill
		        || (winc[0] == 2 || (winc[0] == 1 && winc[2] == 2))
		        || (winc[1] == 2 || (winc[1] == 1 && winc[2] == 2))
		        || (winc[2] == 3 || (winc[0] == 1 && winc[1] == 1 && winc[2] == 1)) ) {

// 满足结束条件 → DuelEnd
			NetServer::SendPacketToPlayer(players[0], STOC_DUEL_END);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
			NetServer::StopServer();
#else
			duel_stage = DUEL_STAGE_END;
#endif

		} else {

// 未满足条件 → 进入换备牌阶段 (SIDING)
			// 如果先后攻被交换过（选了后攻），恢复原始顺序
			if(players[0] != pplayer[0]) {
				players[0] = pplayer[0];
				players[1] = pplayer[1];
				players[0]->type = 0;
				players[1]->type = 1;
				Deck d = pdeck[0];
				pdeck[0] = pdeck[1];
				pdeck[1] = d;
			}
			ready[0] = false;
			ready[1] = false;
			players[0]->state = CTOS_UPDATE_DECK;
			players[1]->state = CTOS_UPDATE_DECK;
			NetServer::SendPacketToPlayer(players[0], STOC_CHANGE_SIDE);
			NetServer::SendPacketToPlayer(players[1], STOC_CHANGE_SIDE);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::SendPacketToPlayer(*oit, STOC_WAITING_SIDE);
#ifdef YGOPRO_SERVER_MODE
			if(cache_recorder)
				NetServer::SendPacketToPlayer(cache_recorder, STOC_WAITING_SIDE);
			if(replay_recorder)
				NetServer::SendPacketToPlayer(replay_recorder, STOC_WAITING_SIDE);
#endif
			duel_stage = DUEL_STAGE_SIDING;
		}
	}
}

// ============================================================
// Surrender — 玩家投降
// dp : 投降的玩家
// 发送 MSG_WIN(reason=0) 给对方 → 记录 match_result → EndDuel → DuelEndProc
// ============================================================
void SingleDuel::Surrender(DuelPlayer* dp) {
	if(dp->type > 1 || !pduel)
		return;

// 1. 构造投降 MSG_WIN (reason=0)
	unsigned char wbuf[3];
	uint32_t player = dp->type;
	wbuf[0] = MSG_WIN;
	wbuf[1] = 1 - player;
	wbuf[2] = 0;

// 2. 广播胜利消息
	NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, wbuf, 3);
	NetServer::ReSendToPlayer(players[1]);
	for(auto oit = observers.begin(); oit != observers.end(); ++oit)
		NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
	NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif

// 3. 记录赛果（考虑是否交换过先后攻）
	if(players[player] == pplayer[player]) {
		match_result[duel_count++] = 1 - player;
		tp_player = player;
	} else {
		match_result[duel_count++] = player;
		tp_player = 1 - player;
	}

// 4. 终结
	EndDuel();
	DuelEndProc();
	event_del(etimer);
}

// ============================================================
// Analyze — 解析引擎消息并分发
// msgbuffer : 引擎输出的消息缓冲区
// len       : 缓冲区长度
//
// 返回值：0=继续处理, 1=等待玩家响应, 2=决斗结束
//
// 消息分三类：
//   a. 等待响应消息 — 发给指定玩家，暂停循环
//   b. 全广播消息 — 发给双方和观察者
//   c. 遮罩消息 — 对手/观察者看到部分信息被隐藏（code=0）
// ============================================================
int SingleDuel::Analyze(unsigned char* msgbuffer, unsigned int len) {
	unsigned char* offset, *pbufw, *pbuf = msgbuffer;
	int player, count, type;

// 循环解析每条消息
	while (pbuf - msgbuffer < (int)len) {
		offset = pbuf;
		unsigned char engType = BufferIO::Read<uint8_t>(pbuf);
#ifdef YGOPRO_SERVER_MODE
		last_game_msg = engType;
#endif
		switch (engType) {

// ===================== 等待响应消息 =====================
// MSG_RETRY — 重发上一条请求（给 last_response 指定的玩家）
		case MSG_RETRY: {
			WaitforResponse(last_response);
			NetServer::SendBufferToPlayer(players[last_response], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// ===================== MSG_HINT — 根据 hint 类型选择性广播 =====================
		case MSG_HINT: {
			type = BufferIO::Read<uint8_t>(pbuf);
			player = BufferIO::Read<uint8_t>(pbuf);
			BufferIO::Read<int32_t>(pbuf);
			switch (type) {
			// type 1/2/3/5 → 仅发给当前玩家
			case 1:
			case 2:
			case 3:
			case 5: {
				NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
				break;
			}
			// type 4/6/7/8/9/11 → 发给对手 + 观察者
			case 4:
			case 6:
			case 7:
			case 8:
			case 9:
			case 11: {
				NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, offset, pbuf - offset);
				for(auto oit = observers.begin(); oit != observers.end(); ++oit)
					NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
				NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
				break;
			}
			// type 10 → 全广播
			case 10: {
				NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
				NetServer::SendBufferToPlayer(players[1], STOC_GAME_MSG, offset, pbuf - offset);
				for(auto oit = observers.begin(); oit != observers.end(); ++oit)
					NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
				NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
				break;
			}
			}
			break;
		}

// ===================== MSG_WIN — 胜利/平局消息 =====================
		case MSG_WIN: {
			player = BufferIO::Read<uint8_t>(pbuf);
			type = BufferIO::Read<uint8_t>(pbuf);

// 全广播胜利消息
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif

// 记录赛果：player > 1 表示平局(2)，否则根据是否交换过判断胜负
			if(player > 1) {
				match_result[duel_count++] = 2;
				tp_player = 1 - tp_player;
			} else if(players[player] == pplayer[player]) {
				match_result[duel_count++] = player;
				tp_player = 1 - player;
			} else {
				match_result[duel_count++] = 1 - player;
				tp_player = player;
			}
			EndDuel();
			return 2;
		}

// ===================== 选择类消息 — 跳过数据后 WaitforResponse =====================

// MSG_SELECT_BATTLECMD — 选战阶指令
		case MSG_SELECT_BATTLECMD: {
			player = BufferIO::Read<uint8_t>(pbuf);
			// 跳过可选攻击对象列表
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 11;
			// 跳过可选效果列表
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 8 + 2;
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_IDLECMD — 选主阶指令
		case MSG_SELECT_IDLECMD: {
			player = BufferIO::Read<uint8_t>(pbuf);
			// 召唤
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			// 特殊召唤
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			// 放置
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			// 发动
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			// 设置
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			// 阶段切换
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 11 + 3;
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_EFFECTYN — 选是否发动效果
		case MSG_SELECT_EFFECTYN: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 12;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_YESNO — 选是/否
		case MSG_SELECT_YESNO: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 4;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_OPTION — 选选项
		case MSG_SELECT_OPTION: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 4;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_CARD / MSG_SELECT_TRIBUTE — 选卡/选祭品
// 对对手的卡进行遮罩（code 写 0）
		case MSG_SELECT_CARD:
		case MSG_SELECT_TRIBUTE: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 3;
			count = BufferIO::Read<uint8_t>(pbuf);
			int c/*, l, s, ss, code*/;
			for (int i = 0; i < count; ++i) {
				pbufw = pbuf;
				/*code = */BufferIO::Read<int32_t>(pbuf);
				c = BufferIO::Read<uint8_t>(pbuf);
				/*l = */BufferIO::Read<uint8_t>(pbuf);
				/*s = */BufferIO::Read<uint8_t>(pbuf);
				/*ss = */BufferIO::Read<uint8_t>(pbuf);
				// 非己方卡的 code 清零（遮罩）
				if (c != player) BufferIO::Write<int32_t>(pbufw, 0);
			}
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_UNSELECT_CARD — 选/取消选卡
		case MSG_SELECT_UNSELECT_CARD: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 4;
			// selectable 列表 — 对对手的卡进行遮罩
			count = BufferIO::Read<uint8_t>(pbuf);
			int c/*, l, s, ss, code*/;
			for (int i = 0; i < count; ++i) {
				pbufw = pbuf;
				/*code = */BufferIO::Read<int32_t>(pbuf);
				c = BufferIO::Read<uint8_t>(pbuf);
				/*l = */BufferIO::Read<uint8_t>(pbuf);
				/*s = */BufferIO::Read<uint8_t>(pbuf);
				/*ss = */BufferIO::Read<uint8_t>(pbuf);
				if (c != player) BufferIO::Write<int32_t>(pbufw, 0);
			}
			// unselectable 列表 — 同样遮罩
			count = BufferIO::Read<uint8_t>(pbuf);
			for (int i = 0; i < count; ++i) {
				pbufw = pbuf;
				/*code = */BufferIO::Read<int32_t>(pbuf);
				c = BufferIO::Read<uint8_t>(pbuf);
				/*l = */BufferIO::Read<uint8_t>(pbuf);
				/*s = */BufferIO::Read<uint8_t>(pbuf);
				/*ss = */BufferIO::Read<uint8_t>(pbuf);
				if (c != player) BufferIO::Write<int32_t>(pbufw, 0);
			}
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_CHAIN — 选连锁
		case MSG_SELECT_CHAIN: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 9 + count * 14;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_PLACE / MSG_SELECT_DISFIELD — 选放置位置 / 选无效区域
		case MSG_SELECT_PLACE:
		case MSG_SELECT_DISFIELD: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 5;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_POSITION — 选表示形式
		case MSG_SELECT_POSITION: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 5;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_COUNTER — 选指示物
		case MSG_SELECT_COUNTER: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 4;
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 9;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SELECT_SUM — 选总和（数量选择）
		case MSG_SELECT_SUM: {
			pbuf++;
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 6;
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 11;
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 11;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_SORT_CARD — 排卡序（等待玩家拖拽排序）
		case MSG_SORT_CARD: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// ===================== 全广播消息（无需遮罩、不等待响应） =====================

// MSG_CONFIRM_DECKTOP — 确认卡组顶
		case MSG_CONFIRM_DECKTOP: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CONFIRM_EXTRATOP — 确认额外卡组顶
		case MSG_CONFIRM_EXTRATOP: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 7;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for (auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CONFIRM_CARDS — 确认卡牌
// 如果确认的是卡组中的卡，则只发给确认者；否则全广播
		case MSG_CONFIRM_CARDS: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 1;
			count = BufferIO::Read<uint8_t>(pbuf);
			if(pbuf[5] != LOCATION_DECK) {
				// 非卡组来源 → 全广播
				pbuf += count * 7;
				NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
				NetServer::ReSendToPlayer(players[1 - player]);
				for(auto oit = observers.begin(); oit != observers.end(); ++oit)
					NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
				NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			} else {
				// 卡组来源 → 只发给确认者
				pbuf += count * 7;
				NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			}
			break;
		}

// MSG_SHUFFLE_DECK — 洗牌（全广播）
		case MSG_SHUFFLE_DECK: {
			player = BufferIO::Read<uint8_t>(pbuf);
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_SHUFFLE_HAND — 洗手牌
// 主动方看到完整列表，对手看到 code=0 的遮罩列表
		case MSG_SHUFFLE_HAND: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			// 发给主动方（完整数据）
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, (pbuf - offset) + count * 4);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(replay_recorder);
#endif
			// 清零所有 code → 发给对手和观察者
			for(int i = 0; i < count; ++i)
				BufferIO::Write<int32_t>(pbuf, 0);
			NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, offset, pbuf - offset);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(cache_recorder);
#endif
			RefreshHand(player, 0x781fff, 0);
			break;
		}

// MSG_SHUFFLE_EXTRA — 洗额外卡组（逻辑同 SHUFFLE_HAND）
		case MSG_SHUFFLE_EXTRA: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, (pbuf - offset) + count * 4);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(replay_recorder);
#endif
			for (int i = 0; i < count; ++i)
				BufferIO::Write<int32_t>(pbuf, 0);
			NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, offset, pbuf - offset);
			for (auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(cache_recorder);
#endif
			RefreshExtra(player);
			break;
		}

// MSG_REFRESH_DECK — 刷新卡组
		case MSG_REFRESH_DECK: {
			pbuf++;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_SWAP_GRAVE_DECK — 墓地和卡组互换（全广播 + 刷新墓地）
		case MSG_SWAP_GRAVE_DECK: {
			player = BufferIO::Read<uint8_t>(pbuf);
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshGrave(player);
			break;
		}

// MSG_REVERSE_DECK — 反转卡组（全广播）
		case MSG_REVERSE_DECK: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
			deck_reversed = !deck_reversed;
#endif
			break;
		}

// MSG_DECK_TOP — 卡组顶信息（全广播）
		case MSG_DECK_TOP: {
			pbuf += 6;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_SHUFFLE_SET_CARD — 里侧卡混洗
// MZONE 区域刷新前后场，SZONE 区域只刷新魔陷区
		case MSG_SHUFFLE_SET_CARD: {
			unsigned int loc = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			if(loc == LOCATION_MZONE) {
				RefreshMzone(0, 0x181fff, 0);
				RefreshMzone(1, 0x181fff, 0);
			}
			else {
				RefreshSzone(0, 0x181fff, 0);
				RefreshSzone(1, 0x181fff, 0);
			}
			break;
		}

// MSG_NEW_TURN — 新回合
// 刷新所有区域 → 重置时限 → 全广播
		case MSG_NEW_TURN: {
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
#ifdef YGOPRO_SERVER_MODE
			turn_player = BufferIO::Read<uint8_t>(pbuf);
#else
			pbuf++;
#endif
			time_limit[0] = host_info.time_limit;
			time_limit[1] = host_info.time_limit;
#ifdef YGOPRO_SERVER_MODE
			time_compensator[0] = host_info.time_limit;
			time_compensator[1] = host_info.time_limit;
			time_backed[0] = host_info.time_limit;
			time_backed[1] = host_info.time_limit;
#endif
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_NEW_PHASE — 新阶段
// 全广播后刷新所有区域
		case MSG_NEW_PHASE: {
#ifdef YGOPRO_SERVER_MODE
			phase = BufferIO::Read<uint16_t>(pbuf);
#else
			pbuf += 2;
#endif
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
			break;
		}

// ===================== 遮罩消息 — 对手/观察者看到部分信息被隐藏 =====================

// MSG_MOVE — 卡移动
// 目标方看到完整 code，对手如果卡在卡组/手牌或是里侧 → 看到 code=0
		case MSG_MOVE: {
			pbufw = pbuf;
			int pc = pbuf[4];  // 原控制者
			int pl = pbuf[5];  // 原位置
			/*int ps = pbuf[6];*/
			/*int pp = pbuf[7];*/
			int cc = pbuf[8];  // 新控制者
			int cl = pbuf[9];  // 新位置
			int cs = pbuf[10]; // 新序号
			int cp = pbuf[11]; // 新表示形式
			pbuf += 16;
			// 发给目标方（完整数据）
			NetServer::SendBufferToPlayer(players[cc], STOC_GAME_MSG, offset, pbuf - offset);
			// 遮罩条件：不是墓地/超量素材 且 (是卡组/手牌 或 里侧) → code=0
			if (!(cl & (LOCATION_GRAVE + LOCATION_OVERLAY)) && ((cl & (LOCATION_DECK + LOCATION_HAND)) || (cp & POS_FACEDOWN)))
				BufferIO::Write<int32_t>(pbufw, 0);
			NetServer::SendBufferToPlayer(players[1 - cc], STOC_GAME_MSG, offset, pbuf - offset);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			// 非叠放且位置/控制者有变化 → 刷新
			if (cl != 0 && (cl & LOCATION_OVERLAY) == 0 && (cl != pl || pc != cc))
				RefreshSingle(cc, cl, cs);
			break;
		}

// MSG_POS_CHANGE — 表示形式变化
// 全广播，如果由里侧变表侧 → RefreshSingle
		case MSG_POS_CHANGE: {
			int cc = pbuf[4];
			int cl = pbuf[5];
			int cs = pbuf[6];
			int pp = pbuf[7];
			int cp = pbuf[8];
			pbuf += 9;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			if((pp & POS_FACEDOWN) && (cp & POS_FACEUP))
				RefreshSingle(cc, cl, cs);
			break;
		}

// MSG_SET — 盖卡（code 清零后全广播）
		case MSG_SET: {
			BufferIO::Write<int32_t>(pbuf, 0);
			pbuf += 4;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_SWAP — 两张卡位置互换（全广播后刷新两张卡）
		case MSG_SWAP: {
			int c1 = pbuf[4];
			int l1 = pbuf[5];
			int s1 = pbuf[6];
			int c2 = pbuf[12];
			int l2 = pbuf[13];
			int s2 = pbuf[14];
			pbuf += 16;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshSingle(c1, l1, s1);
			RefreshSingle(c2, l2, s2);
			break;
		}

// MSG_FIELD_DISABLED — 区域失效（全广播）
		case MSG_FIELD_DISABLED: {
			pbuf += 4;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_SUMMONING — 即将召唤（全广播）
		case MSG_SUMMONING: {
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_SUMMONED — 召唤完成（全广播后刷新全场）
		case MSG_SUMMONED: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			break;
		}

// MSG_SPSUMMONING — 即将特殊召唤
// 控制者看到完整代码，若里侧则对手看到 code=0
		case MSG_SPSUMMONING: {
			pbufw = pbuf;
			int cc = pbuf[4];
			/*int cl = pbuf[5];*/
			/*int cs = pbuf[6];*/
			int cp = pbuf[7];
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[cc], STOC_GAME_MSG, offset, pbuf - offset);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(replay_recorder);
#endif
			if (cp & POS_FACEDOWN)
				BufferIO::Write<int32_t>(pbufw, 0);
			NetServer::SendBufferToPlayer(players[1 - cc], STOC_GAME_MSG, offset, pbuf - offset);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(cache_recorder);
#endif
			break;
		}

// MSG_SPSUMMONED — 特殊召唤完成（全广播后刷新全场）
		case MSG_SPSUMMONED: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			break;
		}

// MSG_FLIPSUMMONING — 即将反转召唤（先刷新单卡获得里侧信息，再全广播）
		case MSG_FLIPSUMMONING: {
			RefreshSingle(pbuf[4], pbuf[5], pbuf[6]);
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_FLIPSUMMONED — 反转召唤完成（全广播后刷新全场）
		case MSG_FLIPSUMMONED: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			break;
		}

// MSG_CHAINING — 连锁发动（全广播）
		case MSG_CHAINING: {
			pbuf += 16;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CHAINED — 连锁已加入（全广播后刷新全场）
		case MSG_CHAINED: {
			pbuf++;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
			break;
		}

// MSG_CHAIN_SOLVING — 连锁处理中（全广播）
		case MSG_CHAIN_SOLVING: {
			pbuf++;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CHAIN_SOLVED — 连锁处理完毕（全广播后刷新全场）
		case MSG_CHAIN_SOLVED: {
			pbuf++;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
			break;
		}

// MSG_CHAIN_END — 连锁结束（全广播后刷新全场）
		case MSG_CHAIN_END: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			RefreshSzone(0);
			RefreshSzone(1);
			RefreshHand(0);
			RefreshHand(1);
			break;
		}

// MSG_CHAIN_NEGATED — 连锁被无效（全广播）
		case MSG_CHAIN_NEGATED: {
			pbuf++;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CHAIN_DISABLED — 连锁被禁用（全广播）
		case MSG_CHAIN_DISABLED: {
			pbuf++;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CARD_SELECTED — 卡已被选（仅服务端内部记录，不转发给客户端）
		case MSG_CARD_SELECTED: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 4;
			break;
		}

// MSG_RANDOM_SELECTED — 随机选择（发给选择者 + 全广播）
		case MSG_RANDOM_SELECTED: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 4;
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_BECOME_TARGET — 成为对象（全广播）
		case MSG_BECOME_TARGET: {
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count * 4;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_DRAW — 抽卡
// 抽卡者看到完整代码，对手只看到 0x80 位标记的（公开的）卡
		case MSG_DRAW: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbufw = pbuf;
			pbuf += count * 4;
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(replay_recorder);
#endif
			for (int i = 0; i < count; ++i) {
				if(!(pbufw[3] & 0x80))
					BufferIO::Write<int32_t>(pbufw, 0);
				else
					pbufw += 4;
			}
			NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, offset, pbuf - offset);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayer(cache_recorder);
#endif
			break;
		}

// ===================== 纯广播消息（无遮罩，不等待响应） =====================

// MSG_DAMAGE — 伤害（全广播）
		case MSG_DAMAGE: {
			pbuf += 5;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_RECOVER — 回复（全广播）
		case MSG_RECOVER: {
			pbuf += 5;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_EQUIP — 装备（全广播）
		case MSG_EQUIP: {
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_LPUPDATE — LP 更新（全广播）
		case MSG_LPUPDATE: {
			pbuf += 5;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_UNEQUIP — 解除装备（全广播）
		case MSG_UNEQUIP: {
			pbuf += 4;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CARD_TARGET — 指定对象（全广播）
		case MSG_CARD_TARGET: {
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_CANCEL_TARGET — 取消对象（全广播）
		case MSG_CANCEL_TARGET: {
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_PAY_LPCOST — 支付 LP（全广播）
		case MSG_PAY_LPCOST: {
			pbuf += 5;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_ADD_COUNTER — 添加指示物（全广播）
		case MSG_ADD_COUNTER: {
			pbuf += 7;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_REMOVE_COUNTER — 移除指示物（全广播）
		case MSG_REMOVE_COUNTER: {
			pbuf += 7;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_ATTACK — 攻击宣言（全广播）
		case MSG_ATTACK: {
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_BATTLE — 战斗结算（全广播）
		case MSG_BATTLE: {
			pbuf += 26;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_ATTACK_DISABLED — 攻击被无效（全广播）
		case MSG_ATTACK_DISABLED: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_DAMAGE_STEP_START — 伤害步骤开始（全广播 + 刷新怪兽区）
		case MSG_DAMAGE_STEP_START: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			break;
		}

// MSG_DAMAGE_STEP_END — 伤害步骤结束（全广播 + 刷新怪兽区）
		case MSG_DAMAGE_STEP_END: {
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			RefreshMzone(0);
			RefreshMzone(1);
			break;
		}

// MSG_MISSED_EFFECT — 错失时点（仅发给相关玩家）
		case MSG_MISSED_EFFECT: {
			player = pbuf[0];
			pbuf += 8;
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			break;
		}

// MSG_TOSS_COIN — 抛硬币（全广播）
		case MSG_TOSS_COIN: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_TOSS_DICE — 掷骰子（全广播）
		case MSG_TOSS_DICE: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += count;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_ROCK_PAPER_SCISSORS — 猜拳（等待玩家响应）
		case MSG_ROCK_PAPER_SCISSORS: {
			player = BufferIO::Read<uint8_t>(pbuf);
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_HAND_RES — 猜拳结果（全广播）
		case MSG_HAND_RES: {
			pbuf += 1;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for (auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
			break;
		}

// MSG_ANNOUNCE_RACE — 宣言种族（等待选择者响应）
		case MSG_ANNOUNCE_RACE: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 5;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_ANNOUNCE_ATTRIB — 宣言属性（等待选择者响应）
		case MSG_ANNOUNCE_ATTRIB: {
			player = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 5;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_ANNOUNCE_CARD / MSG_ANNOUNCE_NUMBER — 宣言卡名/数字（等待选择者响应）
		case MSG_ANNOUNCE_CARD:
		case MSG_ANNOUNCE_NUMBER: {
			player = BufferIO::Read<uint8_t>(pbuf);
			count = BufferIO::Read<uint8_t>(pbuf);
			pbuf += 4 * count;
			WaitforResponse(player);
			NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, offset, pbuf - offset);
			return 1;
		}

// MSG_CARD_HINT — 卡片提示（全广播）
		case MSG_CARD_HINT: {
			pbuf += 9;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_PLAYER_HINT — 玩家提示（全广播）
		case MSG_PLAYER_HINT: {
			pbuf += 6;
			NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
			NetServer::ReSendToPlayer(players[1]);
			for(auto oit = observers.begin(); oit != observers.end(); ++oit)
				NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
			NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			break;
		}

// MSG_MATCH_KILL — Match 斩杀
// 仅在 match_mode 时生效，记录 match_kill 卡号，全广播
		case MSG_MATCH_KILL: {
			int code = BufferIO::Read<int32_t>(pbuf);
			if(match_mode) {
				match_kill = code;
				NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, offset, pbuf - offset);
				NetServer::ReSendToPlayer(players[1]);
				for(auto oit = observers.begin(); oit != observers.end(); ++oit)
					NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
				NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
			}
			break;
		}
		}
	}
	return 0;
}

// ============================================================
// GetResponse — 处理玩家响应
// dp    : 响应的玩家
// pdata : 响应数据
// len   : 数据长度
//
// 将响应写入回放、提交给引擎，扣除时限，继续 Process
// ============================================================
void SingleDuel::GetResponse(DuelPlayer* dp, unsigned char* pdata, unsigned int len) {
	unsigned char resb[SIZE_RETURN_VALUE]{};

// 1. 截断响应数据到最大返回长度
	if (len > SIZE_RETURN_VALUE)
		len = SIZE_RETURN_VALUE;

// 2. 写入回放 + 提交给引擎
	std::memcpy(resb, pdata, len);
	last_replay.Write<uint8_t>(len);
	last_replay.WriteData(resb, len);
	set_responseb(pduel, resb);
	players[dp->type]->state = 0xff;

// 3. 扣减时限
	if(host_info.time_limit) {
		if(time_limit[dp->type] >= time_elapsed)
			time_limit[dp->type] -= time_elapsed;
		else time_limit[dp->type] = 0;
		time_elapsed = 0;
#ifdef YGOPRO_SERVER_MODE
		// 服务端模式：如果还有备用时间且剩余时间不足，自动补充（每次+1秒）
		if(time_backed[dp->type] > 0 && time_limit[dp->type] < host_info.time_limit && NetServer::IsCanIncreaseTime(last_game_msg, pdata, len)) {
			++time_limit[dp->type];
			++time_compensator[dp->type];
			--time_backed[dp->type];
		}
#endif
	}

// 4. 继续主循环
	Process();
}

// ============================================================
// EndDuel — 结束决斗（清理引擎）
//
// 结束回放录制 → 组装回放数据 → 发送给所有人 → end_duel(pduel) → pduel = 0
// ============================================================
void SingleDuel::EndDuel() {
	if(!pduel)
		return;

// 1. 结束回放录制，组装回放缓冲区
	last_replay.EndRecord();
	std::vector<unsigned char> replay_buffer;
	replay_buffer.reserve(sizeof last_replay.pheader + last_replay.comp_size);
	BufferIO::VectorWrite(replay_buffer, last_replay.pheader);
	BufferIO::VectorWriteBlock(replay_buffer, last_replay.comp_data, last_replay.comp_size);

// 2. 发回放给双方
	NetServer::SendBufferToPlayer(players[0], STOC_REPLAY, replay_buffer.data(), replay_buffer.size());
	NetServer::ReSendToPlayer(players[1]);

#ifdef YGOPRO_SERVER_MODE
// 3a. 服务端模式：根据 REPLAY_MODE 决定是否发给观察者
	if(!(replay_mode & REPLAY_MODE_WATCHER_NO_SEND)) {
		for(auto oit = observers.begin(); oit != observers.end(); ++oit)
			NetServer::ReSendToPlayer(*oit);
		NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
	} else {
		NetServer::ReSendToPlayer(replay_recorder);
	}
#else
// 3b. 非服务端模式：发给所有观察者
	for(auto oit = observers.begin(); oit != observers.end(); ++oit)
		NetServer::ReSendToPlayer(*oit);
#endif //YGOPRO_SERVER_MODE

// 4. 销毁引擎实例
	end_duel(pduel);
	event_del(etimer);
	pduel = 0;
}

// ============================================================
// WaitforResponse — 等待玩家响应
// playerid : 等待的玩家（0 或 1）
//
// 记录 last_response → 给对方发 MSG_WAITING → 设置时限状态
// 有时限：state = CTOS_TIME_CONFIRM；无时限：state = CTOS_RESPONSE
// ============================================================
void SingleDuel::WaitforResponse(int playerid) {
	last_response = playerid;

// 1. 通知对手：正在等待对方操作
	unsigned char msg = MSG_WAITING;
	NetServer::SendPacketToPlayer(players[1 - playerid], STOC_GAME_MSG, msg);

// 2. 有时限 → 发 TimeLimit 包，设状态为 TIME_CONFIRM
	if(host_info.time_limit) {
		STOC_TimeLimit sctl;
		sctl.player = playerid;
		sctl.left_time = time_limit[playerid];
		NetServer::SendPacketToPlayer(players[0], STOC_TIME_LIMIT, sctl);
		NetServer::SendPacketToPlayer(players[1], STOC_TIME_LIMIT, sctl);
		players[playerid]->state = CTOS_TIME_CONFIRM;
	} else
		// 无时限 → 直接设为 RESPONSE 状态
		players[playerid]->state = CTOS_RESPONSE;
}

#ifdef YGOPRO_SERVER_MODE

// ============================================================
// RequestField — 向断线重连玩家发送当前完整战场状态
// dp : 请求战场信息的玩家
//
// 依次发送：MSG_START → MSG_NEW_TURN(×2 if turn_player==1) → MSG_NEW_PHASE
// → query_field_info → Refresh 各区域 → DECK_TOP（如果反转/表侧）
// → TimeLimit → STOC_FIELD_FINISH
// ============================================================
void SingleDuel::RequestField(DuelPlayer* dp) {
	if(dp->type > 1)
		return;

	uint8_t player = dp->type;
	NetServer::SendPacketToPlayer(dp, STOC_DUEL_START);

	uint8_t buf[1024];
	uint8_t* temp_buf = buf;
	// 便捷 lambda：将消息写入 buf 后直接发送到 dp
	auto WriteMsg = [&](const std::function<void(uint8_t*&)> &writer) {
		temp_buf = buf;
		writer(temp_buf);
		NetServer::SendBufferToPlayer(dp, STOC_GAME_MSG, buf, temp_buf - buf);
	};

// 1. MSG_START — 重置战场状态
	WriteMsg([&](uint8_t*& pbuf) {
		BufferIO::Write<uint8_t>(pbuf, MSG_START);
		BufferIO::Write<uint8_t>(pbuf, player);
		BufferIO::Write<uint8_t>(pbuf, host_info.duel_rule);
		BufferIO::Write<int32_t>(pbuf, host_info.start_lp);
		BufferIO::Write<int32_t>(pbuf, host_info.start_lp);
		BufferIO::Write<uint16_t>(pbuf, 0);
		BufferIO::Write<uint16_t>(pbuf, 0);
		BufferIO::Write<uint16_t>(pbuf, 0);
		BufferIO::Write<uint16_t>(pbuf, 0);
	});

// 2. MSG_NEW_TURN — 重建回合（如果 turn_player==1 则发两回合）
	uint8_t newturn_count = (turn_player == 1) ? 2 : 1;
	for (uint8_t i = 0; i < newturn_count; ++i) {
		WriteMsg([&](uint8_t*& pbuf) {
			BufferIO::Write<uint8_t>(pbuf, MSG_NEW_TURN);
			BufferIO::Write<uint8_t>(pbuf, i);
		});
	}

// 3. MSG_NEW_PHASE — 当前阶段
	WriteMsg([&](uint8_t*& pbuf) {
		BufferIO::Write<uint8_t>(pbuf, MSG_NEW_PHASE);
		BufferIO::Write<uint16_t>(pbuf, phase);
	});

// 4. 发送战场信息 (query_field_info)
	WriteMsg([&](uint8_t*& pbuf) {
		auto length = query_field_info(pduel, pbuf);
		pbuf += length;
	});

// 5. 刷新所有区域（先对手后自己）
	RefreshMzone(1 - player, 0xefffff, 0, dp);
	RefreshMzone(player, 0xefffff, 0, dp);
	RefreshSzone(1 - player, 0xefffff, 0, dp);
	RefreshSzone(player, 0xefffff, 0, dp);
	RefreshHand(1 - player, 0xefffff, 0, dp);
	RefreshHand(player, 0xefffff, 0, dp);
	RefreshGrave(1 - player, 0xefffff, 0, dp);
	RefreshGrave(player, 0xefffff, 0, dp);
	RefreshExtra(1 - player, 0xefffff, 0, dp);
	RefreshExtra(player, 0xefffff, 0, dp);
	RefreshRemoved(1 - player, 0xefffff, 0, dp);
	RefreshRemoved(player, 0xefffff, 0, dp);

// 6. 如果卡组被反转，发送 MSG_REVERSE_DECK
	if(deck_reversed)
		WriteMsg([&](uint8_t*& pbuf) {
			BufferIO::Write<uint8_t>(pbuf, MSG_REVERSE_DECK);
		});

// 7. 卡组顶信息 — 仅当反转或表侧时发送
	uint8_t query_buffer[SIZE_QUERY_BUFFER];
	for(uint8_t i = 0; i < 2; ++i) {
		auto qlen = query_field_card(pduel, i, LOCATION_DECK, QUERY_CODE | QUERY_POSITION, query_buffer, 0);
		if(!qlen)
			continue;
		uint8_t *qbuf = query_buffer;
		uint32_t code = 0;
		uint32_t position = 0;
		// 遍历找到最后一张卡（卡组顶）
		while(qbuf < query_buffer + qlen) {
			auto clen = BufferIO::Read<int32_t>(qbuf);
			if(qbuf + clen - 4 == query_buffer + qlen) {
				code = *(uint32_t*)(qbuf + 4);
				position = GetPosition(qbuf, 8);
			}
			qbuf += clen - 4;
		}
		// 表侧 → 标记 0x80000000
		if(position & POS_FACEUP)
			code |= 0x80000000;
		if(deck_reversed || position & POS_FACEUP)
			WriteMsg([&](uint8_t*& pbuf) {
				BufferIO::Write<uint8_t>(pbuf, MSG_DECK_TOP);
				BufferIO::Write<uint8_t>(pbuf, i);
				BufferIO::Write<uint8_t>(pbuf, 0);
				BufferIO::Write<int32_t>(pbuf, code);
			});
	}

// 8. 发送时限信息
	/*
	if(dp == players[last_response])
		WaitforResponse(last_response);
	*/
	STOC_TimeLimit sctl;
	sctl.player = 1 - last_response;
	sctl.left_time = time_limit[1 - last_response];
	NetServer::SendPacketToPlayer(dp, STOC_TIME_LIMIT, sctl);
	sctl.player = last_response;
	sctl.left_time = time_limit[last_response] - time_elapsed;
	NetServer::SendPacketToPlayer(dp, STOC_TIME_LIMIT, sctl);

// 9. FIELD_FINISH 标记战场发送完成
	NetServer::SendPacketToPlayer(dp, STOC_FIELD_FINISH);
}
#endif //YGOPRO_SERVER_MODE

// ============================================================
// TimeConfirm — 玩家确认时限
// dp : 确认的玩家（必须是当前 last_response）
//
// 在 host_info.time_limit 有效时，确认后根据 elapsed 扣时：
//   服务端模式：elapsed < 10 且 ≤ compensator → 从补偿时间扣，否则从 time_limit 扣
//   非服务端模式：elapsed < 10 → 免扣（防网络延迟误判）
// ============================================================
void SingleDuel::TimeConfirm(DuelPlayer* dp) {
	if(host_info.time_limit == 0)
		return;
	if(dp->type != last_response)
		return;
	players[last_response]->state = CTOS_RESPONSE;

#ifdef YGOPRO_SERVER_MODE
	if(time_elapsed < 10 && time_elapsed <= time_compensator[dp->type]){
		time_compensator[dp->type] -= time_elapsed;
		time_elapsed = 0;
	}
	else {
		time_limit[dp->type] -= time_elapsed;
		time_elapsed = 0;
	}
#else
	if(time_elapsed < 10)
		time_elapsed = 0;
#endif //YGOPRO_SERVER_MODE
}

// ============================================================
// WriteUpdateData — 辅助：写 MSG_UPDATE_DATA 头并查询区域卡牌
// player    : 查询的玩家 (引擎坐标)
// location  : 区域 (LOCATION_MZONE/SZONE/HAND 等)
// flag      : 查询标志 (QUERY_CODE | QUERY_POSITION | ...)
// qbuf      : 输出缓冲区指针引用（调用后指向数据末尾）
// use_cache : 是否使用引擎缓存
// 返回值    : query_field_card 返回的数据长度
// ============================================================
inline int SingleDuel::WriteUpdateData(int& player, int location, int& flag, unsigned char*& qbuf, int& use_cache) {
	flag |= (QUERY_CODE | QUERY_POSITION);
	BufferIO::Write<uint8_t>(qbuf, MSG_UPDATE_DATA);
	BufferIO::Write<uint8_t>(qbuf, player);
	BufferIO::Write<uint8_t>(qbuf, location);
	int len = query_field_card(pduel, player, location, flag, qbuf, use_cache);
	return len;
}

// ============================================================
// RefreshMzone — 刷新怪兽区
// player    : 查询的玩家
// flag      : 查询标志
// use_cache : 是否使用缓存
// dp        : (服务端模式) 指定的目标玩家，null 表示全体
//
// 逻辑：完整数据发给 owner，里侧卡清零后发给对手和观察者
// ============================================================
#ifdef YGOPRO_SERVER_MODE
void SingleDuel::RefreshMzone(int player, int flag, int use_cache, DuelPlayer* dp)
#else
void SingleDuel::RefreshMzone(int player, int flag, int use_cache)
#endif //YGOPRO_SERVER_MODE
{
	std::vector<unsigned char> query_buffer;
	query_buffer.resize(SIZE_QUERY_BUFFER);
	auto qbuf = query_buffer.data();
	auto len = WriteUpdateData(player, LOCATION_MZONE, flag, qbuf, use_cache);

// 1. 发给 owner（完整数据）
#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[player])
#endif
	NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(replay_recorder);
#endif

// 2. 遍历查询结果，里侧卡清零
	int qlen = 0;
	while(qlen < len) {
		const int clen = BufferIO::Read<int32_t>(qbuf);
		qlen += clen;
		if (clen <= LEN_HEADER)
			continue;
		auto position = GetPosition(qbuf, 8);
		if (position & POS_FACEDOWN)
			std::memset(qbuf, 0, clen - 4);
		qbuf += clen - 4;
	}

// 3. 发给对手 + 观察者（遮罩后数据）
#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[1 - player])
#endif
	NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
if(!dp)
#endif
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::ReSendToPlayer(*pit);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(cache_recorder);
#endif
}

// ============================================================
// RefreshSzone — 刷新魔陷区（逻辑同 RefreshMzone，区域为 LOCATION_SZONE）
// ============================================================
#ifdef YGOPRO_SERVER_MODE
void SingleDuel::RefreshSzone(int player, int flag, int use_cache, DuelPlayer* dp)
#else
void SingleDuel::RefreshSzone(int player, int flag, int use_cache)
#endif //YGOPRO_SERVER_MODE
{
	std::vector<unsigned char> query_buffer;
	query_buffer.resize(SIZE_QUERY_BUFFER);
	auto qbuf = query_buffer.data();
	auto len = WriteUpdateData(player, LOCATION_SZONE, flag, qbuf, use_cache);

#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[player])
#endif
	NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(replay_recorder);
#endif
	int qlen = 0;
	while(qlen < len) {
		const int clen = BufferIO::Read<int32_t>(qbuf);
		qlen += clen;
		if (clen <= LEN_HEADER)
			continue;
		auto position = GetPosition(qbuf, 8);
		if (position & POS_FACEDOWN)
			std::memset(qbuf, 0, clen - 4);
		qbuf += clen - 4;
	}
#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[1 - player])
#endif
	NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
if(!dp)
#endif
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::ReSendToPlayer(*pit);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(cache_recorder);
#endif
}

// ============================================================
// RefreshHand — 刷新手牌（表侧卡可见，里侧卡清零后发给对手和观察者）
// ============================================================
#ifdef YGOPRO_SERVER_MODE
void SingleDuel::RefreshHand(int player, int flag, int use_cache, DuelPlayer* dp)
#else
void SingleDuel::RefreshHand(int player, int flag, int use_cache)
#endif //YGOPRO_SERVER_MODE
{
	std::vector<unsigned char> query_buffer;
	query_buffer.resize(SIZE_QUERY_BUFFER);
	auto qbuf = query_buffer.data();
	auto len = WriteUpdateData(player, LOCATION_HAND, flag, qbuf, use_cache);

#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[player])
#endif
	NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(replay_recorder);
#endif
	int qlen = 0;
	while(qlen < len) {
		const int slen = BufferIO::Read<int32_t>(qbuf);
		qlen += slen;
		if (slen <= LEN_HEADER)
			continue;
		auto position = GetPosition(qbuf, 8);
		if(!(position & POS_FACEUP))
			std::memset(qbuf, 0, slen - 4);
		qbuf += slen - 4;
	}
#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[1 - player])
#endif
	NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
if(!dp)
#endif
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::ReSendToPlayer(*pit);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(cache_recorder);
#endif
}

// ============================================================
// RefreshGrave — 刷新墓地（双方和观察者都可见完整数据）
// ============================================================
#ifdef YGOPRO_SERVER_MODE
void SingleDuel::RefreshGrave(int player, int flag, int use_cache, DuelPlayer* dp)
#else
void SingleDuel::RefreshGrave(int player, int flag, int use_cache)
#endif //YGOPRO_SERVER_MODE
{
	std::vector<unsigned char> query_buffer;
	query_buffer.resize(SIZE_QUERY_BUFFER);
	auto qbuf = query_buffer.data();
	auto len = WriteUpdateData(player, LOCATION_GRAVE, flag, qbuf, use_cache);

#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[0])
#endif
	NetServer::SendBufferToPlayer(players[0], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
	if(!dp || dp == players[1])
		NetServer::SendBufferToPlayer(players[1], STOC_GAME_MSG, query_buffer.data(), len + 3);
if(!dp)
#else
	NetServer::ReSendToPlayer(players[1]);
#endif
	for(auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::ReSendToPlayer(*pit);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
}

// ============================================================
// RefreshExtra — 刷新额外卡组
// 完整数据发给 owner 和 replay_recorder，里侧卡清零后发给对手、观察者和 cache_recorder
// ============================================================
#ifdef YGOPRO_SERVER_MODE
void SingleDuel::RefreshExtra(int player, int flag, int use_cache, DuelPlayer* dp)
#else
void SingleDuel::RefreshExtra(int player, int flag, int use_cache)
#endif //YGOPRO_SERVER_MODE
{
	std::vector<unsigned char> query_buffer;
	query_buffer.resize(SIZE_QUERY_BUFFER);
	auto qbuf = query_buffer.data();
	auto len = WriteUpdateData(player, LOCATION_EXTRA, flag, qbuf, use_cache);

#ifdef YGOPRO_SERVER_MODE
if(!dp || dp == players[player])
#endif
	NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, query_buffer.data(), len + 3);
#ifdef YGOPRO_SERVER_MODE
	if(!dp)
		NetServer::ReSendToPlayer(replay_recorder);
	int qlen = 0;
	while(qlen < len) {
		int clen = BufferIO::Read<int32_t>(qbuf);
		qlen += clen;
		if (clen <= LEN_HEADER)
			continue;
		auto position = GetPosition(qbuf, 8);
		if (position & POS_FACEDOWN)
			memset(qbuf, 0, clen - 4);
		qbuf += clen - 4;
	}
	if(!dp || dp == players[1 - player])
		NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, query_buffer.data(), len + 3);
	if(!dp)
		for(auto pit = observers.begin(); pit != observers.end(); ++pit)
			NetServer::ReSendToPlayer(*pit);
	if(!dp)
		NetServer::ReSendToPlayer(cache_recorder);
#endif //YGOPRO_SERVER_MODE
}

#ifdef YGOPRO_SERVER_MODE
// ============================================================
// RefreshRemoved — 刷新除外区（逻辑同 RefreshExtra，仅服务端模式）
// ============================================================
void SingleDuel::RefreshRemoved(int player, int flag, int use_cache, DuelPlayer* dp) {
	std::vector<unsigned char> query_buffer;
	query_buffer.resize(SIZE_QUERY_BUFFER);
	auto qbuf = query_buffer.data();
	auto len = WriteUpdateData(player, LOCATION_REMOVED, flag, qbuf, use_cache);

	if(!dp || dp == players[player])
		NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, query_buffer.data(), len + 3);
	if(!dp)
		NetServer::ReSendToPlayer(replay_recorder);
	int qlen = 0;
	while(qlen < len) {
		int clen = BufferIO::Read<int32_t>(qbuf);
		qlen += clen;
		if (clen <= LEN_HEADER)
			continue;
		auto position = GetPosition(qbuf, 8);
		if (position & POS_FACEDOWN)
			memset(qbuf, 0, clen - 4);
		qbuf += clen - 4;
	}
	if(!dp || dp == players[1 - player])
		NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, query_buffer.data(), len + 3);
	if(!dp)
		for(auto pit = observers.begin(); pit != observers.end(); ++pit)
			NetServer::ReSendToPlayer(*pit);
	if(!dp)
		NetServer::ReSendToPlayer(cache_recorder);
}
#endif

// ============================================================
// RefreshSingle — 刷新单张卡
// player   : 卡的控制者
// location : 区域
// sequence : 序号
// flag     : 查询标志
//
// 完整数据发给 owner；里侧卡清零（仅保留 QUERY_CODE）后发给对手和观察者
// ============================================================
void SingleDuel::RefreshSingle(int player, int location, int sequence, int flag) {
	flag |= (QUERY_CODE | QUERY_POSITION);
	unsigned char query_buffer[0x1000];
	auto qbuf = query_buffer;
	BufferIO::Write<uint8_t>(qbuf, MSG_UPDATE_CARD);
	BufferIO::Write<uint8_t>(qbuf, player);
	BufferIO::Write<uint8_t>(qbuf, location);
	BufferIO::Write<uint8_t>(qbuf, sequence);
	int len = query_card(pduel, player, location, sequence, flag, qbuf, 0);

// 1. 发给 owner（完整数据）
	NetServer::SendBufferToPlayer(players[player], STOC_GAME_MSG, query_buffer, len + 4);

// 2. 无有效数据 → 不发给对手
	if (len <= LEN_HEADER)
		return;

// 3. 里侧卡 → 清零（仅保留 QUERY_CODE 和 code=0）
	const int clen = BufferIO::Read<int32_t>(qbuf);
	auto position = GetPosition(qbuf, 8);
	if (position & POS_FACEDOWN) {
		BufferIO::Write<int32_t>(qbuf, QUERY_CODE);
		BufferIO::Write<int32_t>(qbuf, 0);
		std::memset(qbuf, 0, clen - 12);
	}
	NetServer::SendBufferToPlayer(players[1 - player], STOC_GAME_MSG, query_buffer, len + 4);
	for (auto pit = observers.begin(); pit != observers.end(); ++pit)
		NetServer::ReSendToPlayer(*pit);
#ifdef YGOPRO_SERVER_MODE
	NetServer::ReSendToPlayers(cache_recorder, replay_recorder);
#endif
}

// ============================================================
// MessageHandler — 引擎日志回调
// 将引擎日志消息转发到 mainGame 的调试消息队列
// ============================================================
uint32_t SingleDuel::MessageHandler(intptr_t fduel, uint32_t type) {
	char msgbuf[1024];
	get_log_message(fduel, msgbuf);
	mainGame->AddDebugMsg(msgbuf);
	return 0;
}

// ============================================================
// SingleTimer — 1 秒定时器回调
// 每秒 time_elapsed++，超时则判 last_response 超时负（reason=0x3）
// 记录 match_result 后 EndDuel + DuelEndProc
// ============================================================
void SingleDuel::SingleTimer(evutil_socket_t fd, short events, void* arg) {
	SingleDuel* sd = static_cast<SingleDuel*>(arg);
	sd->time_elapsed++;

// 1. 超时判断
	if(sd->time_elapsed >= sd->time_limit[sd->last_response] || sd->time_limit[sd->last_response] <= 0) {
		// 构造超时负 MSG_WIN (reason=0x3)
		unsigned char wbuf[3];
		uint32_t player = sd->last_response;
		wbuf[0] = MSG_WIN;
		wbuf[1] = 1 - player;
		wbuf[2] = 0x3;

// 2. 全广播超时消息
		NetServer::SendBufferToPlayer(sd->players[0], STOC_GAME_MSG, wbuf, 3);
		NetServer::ReSendToPlayer(sd->players[1]);
		for(auto oit = sd->observers.begin(); oit != sd->observers.end(); ++oit)
			NetServer::ReSendToPlayer(*oit);
#ifdef YGOPRO_SERVER_MODE
		NetServer::ReSendToPlayers(sd->cache_recorder, sd->replay_recorder);
#endif

// 3. 记录赛果
		if(sd->players[player] == sd->pplayer[player]) {
			sd->match_result[sd->duel_count++] = 1 - player;
			sd->tp_player = player;
		} else {
			sd->match_result[sd->duel_count++] = player;
			sd->tp_player = 1 - player;
		}

// 4. 终结
		sd->EndDuel();
		sd->DuelEndProc();
		event_del(sd->etimer);
		return;
	}

// 5. 未超时 → 继续下一次 tick
	timeval timeout = { 1, 0 };
	event_add(sd->etimer, &timeout);
}

}
