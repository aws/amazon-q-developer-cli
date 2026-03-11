/**
 * 17-pura-vida.tsx — Pura Vida game
 *
 * Run: npx tsx examples/17-pura-vida.tsx
 *
 * Controls: ← → to move, space to jump, q to quit
 */
import React, { useRef, useState, useEffect } from 'react';
import { render, Text, Box, useInput, useApp, useFrames, useStdout } from 'twinki';
import { inflateSync } from 'zlib';

// ── Pixel renderer ───────────────────────────────────────────────────────────

const ESC = '\x1b[';
const RST = `${ESC}0m`;

// Map characters to ANSI 256 color indices
const COLOR_RGB: Record<string, [number,number,number]> = {
	'R': [216,62,32], 'r': [149,35,35], 'p': [216,62,32],
	'A': [45,43,152],
	'S': [241,203,104], 's': [228,134,50], 'k': [77,41,20],
	'W': [245,245,245], 'w': [200,210,230],
	'B': [43,41,149], 'b': [25,25,90],
	'H': [138,67,22], 'h': [77,41,20],
	'D': [7,5,8],
	'Y': [220,180,40], 'O': [180,120,40], 'o': [140,90,30], 'K': [7,5,8], 'G': [34,139,34],
	'g': [0,100,0], 'x': [160,160,160], 'P': [0,128,0],
	'l': [80,200,80], 'n': [255,175,0], 'F': [245,245,245], 'f': [0,128,0], '.': [75,135,220],
	'T': [180,120,60],
	'E': [240,145,15], 'I': [220,100,5], 'J': [165,65,20],
	'L': [245,235,210], 'M': [235,210,180],
	'N': [220,160,170], 'Q': [190,130,140],
	'U': [170,170,170], 'V': [140,140,140], 'X': [250,250,245],
	'a': [214,187,137], 'c': [191,161,123], 'd': [172,141,109],
	'e': [166,122,80], 'i': [150,110,76], 'j': [129,106,97],
	'm': [133,88,60], 'q': [115,85,70], 't': [90,72,74],
	'u': [111,69,48], 'v': [89,55,40], 'y': [59,36,31],
	'C': [210,190,50], '9': [190,20,30],
	'z': [180,200,255], 'Z': [230,240,255],
	'1': [50,80,210], '2': [30,40,140], '3': [220,50,30], '4': [170,30,20],
	'5': [240,200,50], '6': [240,140,30], '7': [240,230,210], '8': [100,60,30],
};

// Half-block renderer: ▀ with fg=top, bg=bottom (true color)
const halfBlock = (top: [number,number,number], bottom: [number,number,number]) =>
	`${ESC}38;2;${top[0]};${top[1]};${top[2]};48;2;${bottom[0]};${bottom[1]};${bottom[2]}m▀${RST}`;

// ── Sprites ──────────────────────────────────────────────────────────────────

const SPRITE_W = 24;
function pad(rows: string[]): string[] {
	return rows.map(r => {
		const d = SPRITE_W - r.length, l = Math.floor(d / 2);
		return '.'.repeat(l) + r + '.'.repeat(d - l);
	});
}

// ── Sprites ──────────────────────────────────────────────────────────────────

// CR = Costa Rican character — extracted from ticoGame.png (34x37 grid, 8px pixel size)
// R/r=cap, A=brim, S/s/k=skin, W/w=shirt, B/b=pants, H/h=shoes, D=outline, .=transparent
const CR_IDLE = [
	'........................',
	'........................',
	'........................',
	'........................',
	'.........DDDDDDD........',
	'.......DDDRRRRRrkD......',
	'......DrkRSSSRRkDRk.....',
	'....BDrsrSSRRRRRkrRRD...',
	'....DrRrrRRRRRRRRrrrrD..',
	'....DRRrrRRRRRRRRrrrrD..',
	'....DRRrrRRRRRRRRrrrrrD.',
	'.DDBAAABbDDDDkkkRrrrrrD.',
	'.DBAAABBBDDDDDDDDDDDrrD.',
	'DBBABbbDDHsDDDSSDDDDDrD.',
	'DDDDDDDRssSDDDSsDDDDDDD.',
	'...DRRDSSSDSkkSDSSsDSsD.',
	'...DssDSSsDSSSSDSSSDSsD.',
	'...DDSssSSDSSSSDSSSDSsD.',
	'....DDssSSsSSSSSSsssSDD.',
	'.....DksSSSSSSSSSsssDk..',
	'......DHSSSDDDSSSssDw...',
	'.......DDsSSSSssskD.....',
	'.....AkwwDsssssDDAwD....',
	'....wDwWWwADDDDAwwWWBD..',
	'...wDbwWWWWwwwwWWWWWwD..',
	'...kDWWWAWWWWWWWwAWWWAD.',
	'...kDkSDwWWWWWWWwBksHSD.',
	'...kkSSDwWWWWWWWwwADSSD.',
	'..wHSSSDwWWWWWWWwwADSSsD',
	'..wHsSsDbBBDDDbBBbDDsSsD',
	'..ADHsDbBBBbbDBBBbbDDskD',
	'....DkDBBBBBBDBBBBbbDkD.',
	'......DBBBBBDwbBBBBbD...',
	'......DBwwAD..DAAAAAD...',
	'......DSSSsD..DRSSSsD...',
	'....kDsDbDDD..DDDHDDsD..',
	'...DkHHHDkDw..wDkDHHHkD.',
	'...DkHHHkkD....DkkHHHHD.',
	'...DDDDDDD......HDDDDDD.',
];

const CR_WALK1 = [
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'.........DDDDDDD........',
	'.......DDkRRRRrrDD......',
	'......DrDRSSRRkDRrD.....',
	'.....DrRrSSRRRRRrRRD....',
	'....DrRrRRRRRRRRRrrrD...',
	'....DRRrRRRRRRRRRrrrD...',
	'....DRrrRRRRRRRRRrrrrD..',
	'..bBAABBDDDDDDDrRrrrrD..',
	'.DAAABBBDDDDDDDDDDDrrD..',
	'DBAABbbDsSDDDSsDDDDDDD..',
	'DDDDDDDSSSDSDSSSDDDDDD..',
	'....DsDSSSSSDSSSDDsskD..',
	'....DSDSSSSSDSSSDDSskDw.',
	'....DSSSSSSSSSSssSSHDD..',
	'....wsSSSSSSSSSsssDDD...',
	'.....DSSSDDDSSsskDDD....',
	'......DsSSSSSskDbbDD....',
	'.......DDssssDDwwWwD....',
	'.......DDDDDDAwWWWWwD...',
	'......DWwADDDWWWWWWWwD..',
	'.....DsSbWWwwWWwbSWDDDD.',
	'...DDSsDAWWWWWWwbDssSsD.',
	'..DSSssDwWWWWWWwwwDsSSsD',
	'..DSSsDDwWWWWWWwwwDSSSsD',
	'..DSSsDDDbDDDBBBbDDSSSsD',
	'...wsDDDDbbbBBbbbbDDssDD',
	'.....DDkDbBAAbbbbbbDDDD.',
	'....DkHHkDSRbDbbbbbbDk..',
	'....DHRkkksssDDDBBBDHD..',
	'....DDBBkkkDD...DHkkkD..',
	'.....DDbkkkD....kDbHkD..',
	'......DbkHk....wDkHHDH..',
	'.......HDDD....wDDDDD...',
];

const CR_WALK2 = [
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'.........DDDDDDD........',
	'.......DDkRRRRrrDDD.....',
	'......DRDRSSRRkDkRDA....',
	'.....DRRRSSRRRRRkrRkD...',
	'....DrRrRRRRRRRRRrrrrD..',
	'....DRRrRRRRRRRRRrrrrD..',
	'...DDRrrRRRRRRRRRrrrrrD.',
	'.DDAAABBDDDDDDDDRrrrrrD.',
	'.DAAABBbkHDDDDDDDDDrrrD.',
	'DBBBBbbHsSDDDDsDDDDDDkD.',
	'DDDDDHDSSSDsDDSSsDDDDDD.',
	'...wDsDSSSSSDDSSsDHsskD.',
	'....DSDSSSSSDDSSsDHsskD.',
	'....DSSSSSSSkHSSssSskD..',
	'.....HSSSSSSSSSSssDDD...',
	'.....BSSSDDDSSssRDDDk...',
	'......DsSSSSSskDDDD.....',
	'.......DDssssDDDbbDD....',
	'.......DDDDDDDbAWWwDk...',
	'.....wDHAADDDwWWWWWwDD..',
	'.....wkSAWwwwWWwwWWWwDD.',
	'..wkkSsDwWWWWWWwDSWDDDD.',
	'..HSSssDwWWWWWWwwDssSsD.',
	'..HSSsHDwWWWWWWwwwDHSSsD',
	'..wSSsDDwWWWWwwwwwDSSSsD',
	'...DsDDDDbDDbBBBbDDSSskD',
	'....DDDDDbbbBBbbbbDDDDD.',
	'....DDkkbBBBADbbbbbDDw..',
	'...DDHHkDswABDDbbbbbDDk.',
	'...DDHHkDSsRDDDbbBBDkDk.',
	'....DbBbkkDDk...DDbbkDk.',
	'....kDDkkHDD....DDkHkDH.',
	'.....DDbHkD....ADkHHkw..',
	'.......DDD.....BDDDDD...',
];

const CR_THINKING = [
	'.........DDDDDDD........',
	'.......DDrRRRRRrrD......',
	'.....BDrkRSSSRRkDrD.....',
	'.....rRrrSSRRRRRRDRDkw..',
	'....DRRrrRRRRRRRRRrRkDD.',
	'....DRRrrRRRRRRRRRrRrrD.',
	'....DRrrRRRRRRRRRRrrrrDA',
	'.DDDAABbrrrrRRRRRRrrrrrD',
	'.DAAAABBbDDDDDDDkrrrrrrD',
	'DBAABbbbkDDDDDSSDDkkrrrD',
	'DDDDDDDksSDDDDSSDDDDkrrD',
	'....DDRkDSSDkSDSSSDDDDDD',
	'....wsSHDSSSSSDSSSDsSSDD',
	'....wsSHDSSSSSDSSSDsssDD',
	'....wRssSSSSSSsSSssSssDw',
	'.....DHRHHSSSSSSSssHkDD.',
	'......DkSSDSSSSSsskDDD..',
	'.....DSSSSsDsSsssDDDH...',
	'.....DSSSDHDssRDDDkDD...',
	'....wkSSSDDDRRHDDwwDD...',
	'.....RsssDDDDDBwwWWwDD..',
	'....kSsRDwwwwwWWWwWWwwD.',
	'...DsSssDwWWWWWWwAwBbbD.',
	'...DsSssDwWWWWWWwbwbbbD.',
	'...DsSsHbwWWWWWWWwDkSSRD',
	'....DsDDAWWWWWWWWwDkSSsD',
	'.....DDbwWWWWWWWwwDkSSsD',
	'......DbbBbDbBBBbDDSSssD',
	'......DbBbbDbbBBBbDsSssD',
	'......DbBBbbDBBBBBbDssD.',
	'......DBABBbDDBBBBBbDD..',
	'......DBBBBbDDbABBBbD...',
	'.......bBABbDwDBAAABD...',
	'.......DwwAbD..DwwwwDw..',
	'......DDHSsDD..DRSSSkH..',
	'.....DsssDDkDD.DDkkDsHD.',
	'...kDHHHHkkkDD..DkkHHHHD',
	'...DkHHHkkDDD...DDHHHHHD',
	'...DDDDDDDD......kDDDDDD',
];

const CR_JUMP = [
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........DDDDDD..........',
	'......kkRRRRrrDD........',
	'.....rDRSSRRRRDRRDD.....',
	'....DRrSSRRRRRRkRrrk....',
	'...DrRRRRRRRRRRrrrrD....',
	'...DRrRRRRRRRRRrrrrDw...',
	'...DRrRRRRRRRRRrrrrDw...',
	'...DRkkDDDDDDDDDDrrDA...',
	'.DBAABbDDDDSsDDDDDDDA...',
	'DBABbbHDDDHDsSSDDDDDH...',
	'DDDDDsDkSDSDHSSDDSsDH...',
	'....DSkDSSSDDSSDDSsDk...',
	'....DSsDSSSDDSssSSsD....',
	'.....ssSSSSSSSsssDD.....',
	'.....BDSSDDSSsHDDDD.....',
	'.......DSSSSsHDwWWwDb...',
	'.......DDDDDDAwWWWWwbD..',
	'......DDwAwwwWWwDwWSSsD.',
	'....wksDbDWWWWWwwDDSSSsD',
	'....DSsRDbwWWWWWwwDSSSsD',
	'....DSsbBBDwWWWWwDDRSsD.',
	'.....DDBBBBDDbbbDbbDkD..',
	'......DbBBBbDbBBBBBbD...',
	'......DDSAAbbbBBBBBbbD..',
	'.....DDDHSsDbDDbBAAAHD..',
	'.....DRSRDkkDDDDDDSsDkD.',
	'....DHHHHkkkD..DDDDDHkD.',
	'....DkkkkkkD....DkkRHkD.',
	'.....DDDDDD......DkHHkD.',
	'.................wDDDD..',
];

const CR_CROUCH = [
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'........................',
	'.......DDDDDDD..........',
	'.....ADrRRRRrDkDk.......',
	'....DrrRSSRRRrRRrD......',
	'....kRDSSRRRRRrrRrw.....',
	'...DRRrRRRRRRRrrrrk.....',
	'...DRRrRRRRRRRrRRrD.....',
	'...DRrrRDDDDDDDkrrk.....',
	'.DDAABBDDDDksDDDDrk.....',
	'DBAAABDHHDDDsDDDDDD.....',
	'DBbDDHDSSDSDSSDDSskD....',
	'...DDSDSSSSDSSDDSskD....',
	'....DSDSSSSDSssSSDDD....',
	'....DsSSSSSSSSskDDbwD...',
	'.....kSSDDSSsskbwWwwWD..',
	'.....DDSSSSsDDDwwWWwwWD.',
	'......HHDDDDwwDDDDWSDwwD',
	'.....kSSsDwwDDSSSsDDDwwD',
	'.....kSDsDDDDSSSSSssbAAD',
	'.....DsDsSsDDSkSSsDDBBBD',
	'.....DDDDSSsDDDDDDSSDBDD',
	'.....wsskDDDDDssHDDDDDD.',
	'.....DsHHkkkDHRHkkHRkD..',
	'.....DkkkkkDDDkkkkHHkD..',
	'......DDDDDD.DDDDDDDD...',
];

const CR_WALK3 = CR_WALK1;
const M_IDLE = CR_IDLE;
const M_WALK1 = CR_WALK1;
const M_WALK2 = CR_WALK2;
const M_WALK3 = CR_WALK3;
const M_JUMP = CR_JUMP;
const M_CROUCH = CR_CROUCH;

// ── Corgi companion sprites (16w x 23h) ──────────────────────────────────────
// E=orange, I=dark orange, J=brown, L=cream, M=darker cream
// N=pink, Q=dark pink, U=gray, V=dark gray, X=white, D=outline

const CORGI_IDLE = [
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'UJV.....DJ......',
	'DEE....JEED.....',
	'DNJJ...IJND.....',
	'DNQIEXEJQND.....',
	'DQEEEXNEEED.DD..',
	'.IEEEXEEEJ...MMU',
	'.JELEXELEJDDIEMD',
	'.JEDXVLDEJJJJIED',
	'DXEXXDXLELDIEJIV',
	'.DLXDQDXLDMEEEJ.',
	'.DJLLNLLVLMEIEE.',
	'.DMLUUMUXIIEEIE.',
	'..LUMXMXEEIEEEED',
	'..DELMXJEEDEDIED',
	'...MDMMLXMUDDUUD',
	'....XVDLLXD.LXMV',
	'...DJDVDDDV.DDV.',
];

const CORGI_WALK1 = [
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'....J.D.........',
	'...DIDED........',
	'..VJDENE........',
	'..EXEIQE........',
	'.DEEEEEJ....L.D.',
	'.VEDEEEJ....DELD',
	'DDXMEEMEDDDDLIED',
	'VUUMXLEMDIEIIJD.',
	'.VULLMUIJEEEEED.',
	'..DLXLEIEEEEEEI.',
	'...MLMEEEEIJEIJ.',
	'...JMLEEJUUDDJID',
	'..DLULLU..IQD..M',
	'..DMDQMD..DD..DD',
];

const CORGI_WALK2 = [
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'...JV...DD......',
	'..JEJ..DEI......',
	'..NQE.JEQE......',
	'.DJELEEJNE......',
	'.MEEEEEEED......',
	'.VEEVEEII.......',
	'DDXEDEEEJ....LL.',
	'VUXLXELEMDD..IID',
	'.UDDLXLEDJEEDJED',
	'..VLLVXEIEEEEJJD',
	'..MMXLIJEEEEEI..',
	'..JLMLEEIENIEED.',
	'..DIVDEEDUUDJEN.',
	'...UULXUDDDI.XUV',
	'..XM.DDD..DV.DD.',
];

const CORGI_W = 16;
const CORGI_H = 23;

const CORGI_PLAY = [
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'..DJ.....U......',
	'..EED...DED.....',
	'..ENE..EEND.....',
	'..IQJMEEJND.....',
	'..IEXLEEEED.DU..',
	'..EEXEEEIIJJJED.',
	'..DQXEDEEEDEEID.',
	'.DEDXXEEEEDEEE..',
	'.DMMXMLXLUJJEID.',
	'DLDMUULDXMNVVJXD',
	'UDDDUUUNJDDDDLUU',
];

const CORGI_SLEEP = [
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'................',
	'...VV...........',
	'...MV...........',
	'................',
	'...D..I...DD....',
	'..DV.IQEJEEEEJ..',
	'DDLLDENNDEEEEED.',
	'EQEDXEEJIIJEEEEJ',
	'DNDXXEEEJIEMEJED',
	'.IEELJJELDJEEJED',
	'..DDX.MXMLNDXXXD',
	'.DLVD.L.DLMDDLLV',
];

// ── Macaw flight sprites (20w x 17h) ─────────────────────────────────────────
// 1=bright blue, 2=dark blue, 3=red, 4=dark red, 5=yellow, 6=orange, 7=cream, 8=brown beak

const MACAW_1 = [
	'..............D.....',
	'............2.12....',
	'..........D2121D....',
	'..........21121D....',
	'.........221222D....',
	'.........212121.....',
	'........D212158.....',
	'........D212638.....',
	'........D22563D.....',
	'........2225538.....',
	'.........25543D.448.',
	'........22565443774.',
	'.........22484377777',
	'......22113333337DDD',
	'..D2334124343448D...',
	'2222D22DDD8888D.....',
	'..222DD.DD..........',
];

const MACAW_2 = [
	'....................',
	'....................',
	'....................',
	'....................',
	'.........2..........',
	'........222.........',
	'......22111.........',
	'......21212D........',
	'.....2121253........',
	'.....2221553DD.2....',
	'.....D226563D22284..',
	'......22554432337748',
	'.......2264443377777',
	'.....D22224333337DD7',
	'..D2338284333444DD..',
	'212221225444488.....',
	'..1222DDDDD.........',
];

const MACAW_3 = [
	'....................',
	'....................',
	'....................',
	'....................',
	'....................',
	'....................',
	'....................',
	'...............8334.',
	'......2114433443777D',
	'...D31112633333577D7',
	'.212D222163383333DDD',
	'DDD12DD16638D84D8...',
	'......21153DD2D53D..',
	'.....211122.D2222DD.',
	'.....12122...D2D222.',
	'....DD1D1D.....DD212',
	'.....2.2.........D.D',
];

const MACAW_4 = [
	'....................',
	'....................',
	'........D...........',
	'........1D..........',
	'......2.21D.........',
	'......D11212........',
	'.......21153D.......',
	'.......222336D..DD1D',
	'.......225553D.D111.',
	'........225338.2112.',
	'........22664324333.',
	'........D22644337788',
	'.......D211433367787',
	'.....D22133333434DD.',
	'..D2332244434448....',
	'D2222112DD8DDD......',
	'..D222.DD...........',
];

const MACAW_FRAMES = [MACAW_1, MACAW_2, MACAW_3, MACAW_4];
const MACAW_W = 20;
const MACAW_H = 17;

// ── Fire Mario sprites ───────────────────────────────────────────────────────

const FM_IDLE = CR_IDLE;
const FM_WALK1 = CR_WALK1;
const FM_WALK2 = CR_WALK2;
const FM_WALK3 = CR_WALK3;
const FM_JUMP = CR_JUMP;
const FM_CROUCH = CR_CROUCH;

// Fireball — 4×4 spinning
const FIREBALL = [
	['.YD.','YDDY','DYYD','.DY.'],
	['.DY.','DYYD','YDDY','.YD.'],
];

// ── Bowser sprite — 24×20, NES-style ────────────────────────────────────────

const BOWSER_1 = [
	'......TT................',
	'.....TT.TT..............',
	'....TTTT.TTT............',
	'...TTTH.TTT.............',
	'...TTTH.TTTTT...........',
	'TTTHWHTTTTTWH.TTH.......',
	'TTTWTTTTTKWHHHHHHH......',
	'TTTTTTTTTKWHHHHWHHWTW...',
	'W.TTTHHTKK...HHTHHHHW...',
	'.....HTTKKTTTWHHHHHHH...',
	'....HTTTKHKTTTWHHWTHH.T.',
	'...TTTTKKHKKTTWHHHTHHTW.',
	'......TT.TTTTKWHHHHHHH..',
	'....TTTTTTTTTTWHHHWWHH..',
	'....TTTKTTTTTTWHHHTWHH..',
	'.....TTKTTTTTTW.HHHHHT..',
	'.....TTKTTTTTTH..HHHHTW.',
	'........TTTWTTTW..WHHW..',
	'........THWTTTTTW.......',
	'........TTWTTTTTTHH.....',
	'........TTTHTTTTTHHH....',
	'.........TTTTTTTTTTTTT..',
	'........TTTTTTTTTTHTTTT.',
	'............TTTTTT......',
];
const BOWSER_2 = [
	'......TT................',
	'.....TT.TT..............',
	'....TTTT.TTT............',
	'...TTTH.TTT.............',
	'...TTTH.TTTTT...........',
	'TTTHWHTTTTTWH.TTH.......',
	'TTTWTTTTTKWHHHHHHH......',
	'TTTTTTTTTKWHHHHWHHWTW...',
	'W.TTTHHTKK...HHTHHHHW...',
	'.....HTTKKTTTWHHHHHHH...',
	'....HTTTKHKTTTWHHWTHH.T.',
	'...TTTTKKHKKTTWHHHTHHTW.',
	'......TT.TTTTKWHHHHHHH..',
	'....TTTTTTTTTTWHHHWWHH..',
	'....TTTKTTTTTTWHHHTWHH..',
	'.....TTKTTTTTTW.HHHHHT..',
	'.....TTKTTTTTTH..HHHHTW.',
	'........TTTWTTTW..WHHW..',
	'.......TT..TTTTTW.......',
	'........TTWTTTTTTHH.....',
	'.......TTTHTTTTTHHH.....',
	'........TTTTTTTTTTTTT...',
	'........TTTTTTTTTTHTTTT.',
	'............TTTTTT......',
];
const BOWSER_W = 24;
const BOWSER_H = 24;

// Bowser fire breath — 8×3
const BOWSER_FIRE = [
	['YDDDYYY.','YYYYDDDY','YDDDYYY.'],
	['YDDDYYY.','YDDDDDDY','YDDDYYY.'],
];

// ── World tiles ──────────────────────────────────────────────────────────────

// Brick — 10×10, NES staggered mortar (same size as ? block)
// Bamboo block (brick equivalent) — 14×14
const BRICK = [
	'PfPfPfPfPfPfPf',
	'fGfGfGfGfGfGfG',
	'GfGfGfGfGfGfGf',
	'PfPfPfPfPfPfPf',
	'DDDDDDDDDDDDDD',
	'fGfGfGfGfGfGfG',
	'GfGfGfGfGfGfGf',
	'PfPfPfPfPfPfPf',
	'fGfGfGfGfGfGfG',
	'DDDDDDDDDDDDDD',
	'GfGfGfGfGfGfGf',
	'PfPfPfPfPfPfPf',
	'fGfGfGfGfGfGfG',
	'GfGfGfGfGfGfGf',
];

// Coffee crate (? block equivalent) — 14×14
const Q_BLOCK = [
	'..kkkkkkkkkk..',
	'.kHHHHHHHHHHD.',
	'kHssssssssssHD',
	'kHs........sHD',
	'kHs..kkkk..sHD',
	'kHs..k..k..sHD',
	'kHs.....k..sHD',
	'kHs....k...sHD',
	'kHs....k...sHD',
	'kHs........sHD',
	'kHs....k...sHD',
	'kHssssssssssHD',
	'.kHHHHHHHHHHD.',
	'..DDDDDDDDDD..',
];
const Q_HIT = [
	'..kkkkkkkkkk..',
	'.kHHHHHHHHHHD.',
	'kHkkkkkkkkkkHD',
	'kHkHHkkHHkkHHD',
	'kHkkHHkkHHkHHD',
	'kHkHHkkHHkkHHD',
	'kHkkHHkkHHkHHD',
	'kHkHHkkHHkkHHD',
	'kHkkHHkkHHkHHD',
	'kHkHHkkHHkkHHD',
	'kHkkHHkkHHkHHD',
	'kHkkkkkkkkkkHD',
	'.kHHHHHHHHHHD.',
	'..DDDDDDDDDD..',
];
// Goomba — 18×14, exact NES pixel match from ASCII reference
// Terciopelo (fer-de-lance) — 32×18, faces RIGHT
// a-y = brown palette, 9 = tongue red, C = eye yellow
const GOOMBA_1 = [
	'..............................mq',
	'............................jumv',
	'.........................uyuCvye',
	'........................qvmyvume',
	'........................ueiidmii',
	'.......................jvqiacaad',
	'.......................quyveacce',
	'...........ajtuvumtd...ivyiiqjda',
	'......c...dvvuuuidyvvd.eudcded..',
	'....qvvvqtimvvyyecmuuituueccid..',
	'...jvmyyvuveeuyjqmdieiyyveacid..',
	'...vvivyvveeecmtqqqytyyuvmccma..',
	'dacvqtjtueduuuueemmumumuydacj...',
	'tyvvqjadquivvvmcmmmmcmuyvddej...',
	'.jjtj...tmdiymccuvvuceyyeddqj...',
	'........dtmdeammcvyeumdeditjjd..',
	'..........jtqiqmceedmmeittjca...',
	'............adjjttjttjtjjc......',
];
const GOOMBA_2 = [
	'q.........................muqd..',
	'mi......................uuimvd..',
	'ivc...................qvuCddiuc.',
	'ecc...................ummmvmcav.',
	'dac..................cueceeacey.',
	'dm.....j.............dyvaadv99..',
	'c......jq....dvummj..dyqieav999.',
	'.......cy...jvuvmiyvcdvedqqcdv..',
	'.......uv..jmqvyicuvqtmadic.cca',
	'......cvuqvvuemtqeeiiyvdacd..ca',
	'......cuvvuveecivjiiqyvdacj....',
	'.......timumcuumiqvvyvveaaj....',
	'.......dtvymeuumdmmeimvdacj....',
	'.........cjuevyceuueevycadc....',
	'..........ctciidevvecyqcaic....',
	'...........jtecmcmvemcdcetj....',
	'............ajtmecdeidiqtjc....',
	'...............jjttqjttjc......',
];
const GOOMBA_FLAT = [
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'................................',
	'vuyyiidvvvudimmmuiamumuuiaeyyvu.',
	'tmmyytdiymedmvvvvuaduvyyemqcdet.',
	'cvumvytiicmucmyyvdemduvmcmmqqtjd',
	'.ajtuvyvvvtqicemdeiieiiittttjc..',
	'....cjjjtvjdjjjjjjttjjjjc......',
];
const GOOMBA_W = 32;
const COIN_RAW = ['.kk.','kHHk','kHsk','ksHk','ksHk','kHsk','kHHk','.kk.']; // coffee bean

// Coconut projectile — 10×10, two rotation frames
const COCONUT = [
	[
		'...kHHk...',
		'..kHHHHHk.',
		'.kHsHHsHHk',
		'kHHHsssHHHk',
		'kHHHHHHHHk',
		'kHsHHHHsHk',
		'kHHsHHsHHk',
		'.kHHHHHHk.',
		'..kHHHHk..',
		'...kkkk...',
	],
	[
		'...kHHk...',
		'..kHHHHHk.',
		'.kHHsHHsHk',
		'kHHHsssHHHk',
		'kHHHHHHHHk',
		'kHsHHHHsHk',
		'kHHsHHsHHk',
		'.kHHHHHHk.',
		'..kHHHHk..',
		'...kkkk...',
	],
];

// Fetch ball — red recolor of coconut, 10×10, two rotation frames
// 3=bright red, 4=dark red, 9=deep red, D=outline
const FETCH_BALL = [
	[
		'...D33D...',
		'..D33333D.',
		'.D393393D.',
		'D3339993D.',
		'D333333D..',
		'D393333D..',
		'D339339D..',
		'.D33333D..',
		'..D333D...',
		'...DDDD...',
	],
	[
		'...D33D...',
		'..D33333D.',
		'.D339339D.',
		'D3339993D.',
		'D333333D..',
		'D393333D..',
		'D339339D..',
		'.D33333D..',
		'..D333D...',
		'...DDDD...',
	],
];

// Pipe — 16px wide cap overhangs body, highlight stripe
const PIPE_CAP = [
	'KppPPPPPPPPPPppK',
	'KplPPlPPlPPlPpPK',
	'KplPPlPPlPPlPpPK',
];
const PIPE_BODY = [
	'..KplPPPPPPlPK..',
	'..KplPPPPPPlPK..',
];

// Log obstacle — extracted from logs.png, remapped to existing palette
// Cap: tree-ring cross-section; Body: bark texture with knots, moss, grain
const LOG_CAP = [
	'ykvvuuTTOOaaOOaaOOTTuuvkyy',
	'vuuTOaacddccddccddcaOTuuvv',
	'uTOacddceeddaaddeeddcaOTuu',
	'vuuTOaacddccddccddcaOTuuvv',
	'ykvvuuTTOOaaOOaaOOTTuuvkyy',
];
const LOG_BODY = [
	'.ykvuTOuvkyvTOuykvuTOuvky.',
	'.kvyuOTuvkyuTOukvyuOTukvk.',
	'.ykvuTOTvkyvTOuykvuTOTvky.',
	'.vkyuO8uykvu8OukvyuO8uykv.',
	'.ykuuTOuvkyvTOuykvuTOuuky.',
	'.kvyvOTukvyuTOukvyvOTukvk.',
	'.ykvuTguvkyvTguykvuTguvky.',
	'.vkyuOTuykvuTOukvyuOTuykv.',
	'.ykvvTOuvkyvTOuykvvTOuvky.',
	'.kvyuOTTvkyuTOukvyuOTTkvk.',
	'.ykvu8OuvkyvTOuykvuTO8vky.',
	'.vkyvOTuykvuTOukvyvOTuykv.',
	'.ykvuTOuvkyvTOTykvuTOuvky.',
	'.kvyuOTukvyuTOukvyuOTukvk.',
];

// Cloud — bigger, fluffier NES-style
const CLOUD = [
	'....www.....',
	'..wwwwwww...',
	'.wwwwwwwww..',
	'wwwwwwwwwww.',
	'.wwwwwwwwwww',
	'..wwwwwwww..',
];

// Flag pole — ball on top, pole, Costa Rica flag (blue-white-red-white-blue)
const FLAG_BALL = ['..Y..', '.YYY.', '..Y..'];
// Costa Rica flag — 14 wide, 10 tall, 6 wave frames (B=blue, F=white, 3=red)
const FLAG_FRAMES = [
	['KBBBBBBBBBBBBB','K.BBBBBBBBBBBB','KFFFFFFFFFFFF.','K3333333333333','K.333333333333','K3333333333333','K.33333333333.','KFFFFFFFFFFFF.','K.BBBBBBBBBBBB','KBBBBBBBBBBBBB'],
	['K.BBBBBBBBBBB.','KBBBBBBBBBBBB.','K.FFFFFFFFFFF.','K.333333333333','K33333333333..','K.33333333333.','K333333333333.','K.FFFFFFFFFFF.','KBBBBBBBBBBBB.','K.BBBBBBBBBBB.'],
	['K..BBBBBBBBBB.','K.BBBBBBBBBB..','K..FFFFFFFFFF.','K.333333333...','K3333333333...','.K333333333...','.K33333333333.','K..FFFFFFFFFF.','K.BBBBBBBBBB..','K..BBBBBBBBBB.'],
	['K.BBBBBBBBBBB.','K..BBBBBBBBBBB','K.FFFFFFFFFFF.','K333333333333.','K.333333333333','K333333333333.','K.333333333333','K.FFFFFFFFFFF.','K..BBBBBBBBBBB','K.BBBBBBBBBBB.'],
	['KBBBBBBBBBBBB.','K.BBBBBBBBBBB.','KFFFFFFFFFFF..','K33333333333..','K.333333333333','K33333333333..','K.333333333333','KFFFFFFFFFFF..','K.BBBBBBBBBBB.','KBBBBBBBBBBBB.'],
	['KBBBBBBBBBBBBB','KBBBBBBBBBBBB.','KFFFFFFFFFFFF.','K333333333333.','K3333333333333','K333333333333.','K3333333333333','KFFFFFFFFFFFF.','KBBBBBBBBBBBB.','KBBBBBBBBBBBBB'],
];
const FLAG_POLE_LINE = ['K'];
const FLAG_BASE = ['KKKKK', 'KKKKK'];

// ── Background (128-color indexed, zlib+base64) ──────────────────────────────
const BG_TILE_W = 400;
const BG_DATA = "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";

const _bgBuf = inflateSync(Buffer.from(BG_DATA, 'base64'));
const _bgPal: [number,number,number][] = [];
for (let i = 0; i < 128; i++) _bgPal.push([_bgBuf[i*3]!, _bgBuf[i*3+1]!, _bgBuf[i*3+2]!]);
const _bgIdx = _bgBuf.subarray(384);
const BG_RGB: [number,number,number][][] = [];
for (let y = 0; y < 80; y++) {
	const row: [number,number,number][] = [];
	for (let x = 0; x < BG_TILE_W; x++) row.push(_bgPal[_bgIdx[y * BG_TILE_W + x]!]!);
	BG_RGB.push(row);
}

// ── Scene 3 background tile (jungle/volcano/waterfall) ───────────────────────
const BG_DATA3 = "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";
const _bg3Buf = inflateSync(Buffer.from(BG_DATA3, 'base64'));
const _bg3Pal: [number,number,number][] = [];
for (let i = 0; i < 128; i++) _bg3Pal.push([_bg3Buf[i*3]!, _bg3Buf[i*3+1]!, _bg3Buf[i*3+2]!]);
const _bg3Idx = _bg3Buf.subarray(384);
const BG_RGB3: [number,number,number][][] = [];
for (let y = 0; y < 80; y++) {
	const row: [number,number,number][] = [];
	for (let x = 0; x < BG_TILE_W; x++) row.push(_bg3Pal[_bg3Idx[y * BG_TILE_W + x]!]!);
	BG_RGB3.push(row);
}

// Per-row parallax speed — uniform scroll for seamless panorama
const BG_PARALLAX = 1.0;

// ── Fixed game world dimensions ──────────────────────────────────────────────
// Internal resolution is fixed so layout always works regardless of terminal size.
// The viewport is a window into this world.

const WORLD_W = 1200;
const GAME_H = 80;       // internal grid height
const GROUND_H = 3;
const GROUND_Y = GAME_H - GROUND_H;
const MARIO_H = 39;
const FLOOR_Y = GROUND_Y - MARIO_H;
const PIPE_CAP_H = 3;
const PIPE_W = 16;
const LOG_CAP_H = 5;
const LOG_W = 26;
const BLOCK_H = 14;
const GRAVITY = 0.4;
const JUMP_VEL = -5;
const MOVE_SPEED = 3;

interface Block { x: number; y: number; type: 'brick' | 'question'; hit: boolean }
interface PipeDef { x: number; h: number }
interface CloudDef { x: number; y: number }

// gOff = rows from ground surface to TOP of block
// Mario stands at FLOOR_Y=21, head at 21, jump apex ~21-15=6 rows above floor → y≈6
// Blocks should be around y = GROUND_Y - gOff = 37 - 20 = 17 (head-height when standing)
// That means bottom of block at 17+8=25, Mario head at 21 → 4px gap to jump into
const BLOCKS: Block[] = [
	// Area 1: Beach intro
	{ x: 50, y: 10, type: 'question', hit: false },
	{ x: 80, y: 10, type: 'brick', hit: false },
	{ x: 95, y: 10, type: 'question', hit: false },
	{ x: 110, y: 10, type: 'brick', hit: false },
	// Area 2: Jungle path
	{ x: 190, y: 8, type: 'brick', hit: false },
	{ x: 205, y: 8, type: 'brick', hit: false },
	{ x: 220, y: 8, type: 'question', hit: false },
	{ x: 235, y: 8, type: 'brick', hit: false },
	{ x: 250, y: 8, type: 'brick', hit: false },
	// Area 3: Volcano approach
	{ x: 330, y: 6, type: 'brick', hit: false },
	{ x: 345, y: 6, type: 'question', hit: false },
	{ x: 370, y: 10, type: 'brick', hit: false },
	{ x: 385, y: 10, type: 'question', hit: false },
	{ x: 400, y: 10, type: 'brick', hit: false },
	// Area 4: Cloud forest
	{ x: 470, y: 6, type: 'brick', hit: false },
	{ x: 485, y: 6, type: 'question', hit: false },
	{ x: 500, y: 6, type: 'brick', hit: false },
	{ x: 530, y: 10, type: 'question', hit: false },
	{ x: 560, y: 6, type: 'brick', hit: false },
	{ x: 575, y: 6, type: 'question', hit: false },
	// Area 5: Final stretch
	{ x: 640, y: 10, type: 'brick', hit: false },
	{ x: 655, y: 10, type: 'brick', hit: false },
	{ x: 670, y: 10, type: 'question', hit: false },
	{ x: 685, y: 10, type: 'brick', hit: false },
	{ x: 700, y: 10, type: 'brick', hit: false },
	{ x: 720, y: 8, type: 'question', hit: false },
	{ x: 735, y: 8, type: 'question', hit: false },
	// Area 6: Jungle waterfall (scene3)
	{ x: 830, y: 10, type: 'brick', hit: false },
	{ x: 845, y: 10, type: 'question', hit: false },
	{ x: 860, y: 10, type: 'brick', hit: false },
	{ x: 920, y: 6, type: 'brick', hit: false },
	{ x: 935, y: 6, type: 'question', hit: false },
	{ x: 950, y: 6, type: 'brick', hit: false },
	// Area 7: Volcano ridge
	{ x: 1010, y: 8, type: 'question', hit: false },
	{ x: 1025, y: 8, type: 'brick', hit: false },
	{ x: 1040, y: 8, type: 'question', hit: false },
	{ x: 1080, y: 10, type: 'brick', hit: false },
	{ x: 1095, y: 10, type: 'question', hit: false },
	{ x: 1110, y: 10, type: 'brick', hit: false },
	{ x: 1130, y: 6, type: 'question', hit: false },
];

const PIPES: PipeDef[] = [];

// Logs — same mechanics as pipes (obstacle you can jump on top of)
interface LogDef { x: number; h: number }
const LOGS: LogDef[] = [
	{ x: 200, h: 14 },
	{ x: 450, h: 20 },
	{ x: 680, h: 14 },
	{ x: 880, h: 20 },
	{ x: 1050, h: 14 },
];

const CLOUDS: CloudDef[] = [
	{ x: 10, y: 2 }, { x: 60, y: 3 }, { x: 120, y: 1 },
	{ x: 180, y: 2 }, { x: 240, y: 3 }, { x: 300, y: 1 }, { x: 360, y: 2 },
];

interface Goomba { x: number; y: number; vx: number; dead: number }  // dead=0 alive, >0 squish timer
const INITIAL_GOOMBAS: Goomba[] = [
	{ x: 120, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 260, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 310, y: GROUND_Y - 18, vx: 0.5, dead: 0 },
	{ x: 420, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 500, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 580, y: GROUND_Y - 18, vx: 0.5, dead: 0 },
	{ x: 650, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 730, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	// Scene 3 snakes
	{ x: 850, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 940, y: GROUND_Y - 18, vx: 0.5, dead: 0 },
	{ x: 1020, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
	{ x: 1070, y: GROUND_Y - 18, vx: 0.5, dead: 0 },
	{ x: 1120, y: GROUND_Y - 18, vx: -0.5, dead: 0 },
];

// ── Secret level constants ───────────────────────────────────────────────────

const SECRET_W = 80;  // underground arena width
const SECRET_H = GAME_H;
const SECRET_GROUND_Y = GROUND_Y;
const SECRET_PIPES: number[] = []; // indices of pipes that lead to secret level

interface Fireball { x: number; y: number; vx: number; life: number }
interface BowserFire { x: number; y: number; vx: number; life: number }

// ── Grid helpers ─────────────────────────────────────────────────────────────

function stamp(grid: string[][], sprite: string[], x: number, y: number) {
	const h = grid.length, w = grid[0]!.length;
	for (let r = 0; r < sprite.length; r++) {
		const row = y + r;
		if (row < 0 || row >= h) continue;
		for (let c = 0; c < sprite[r]!.length; c++) {
			const col = x + c;
			if (col < 0 || col >= w || sprite[r]![c] === '.') continue;
			grid[row]![col] = sprite[r]![c]!;
		}
	}
}

function stampOutlined(grid: string[][], sprite: string[], x: number, y: number) {
	const h = grid.length, w = grid[0]!.length;
	// Pass 1: outline — stamp 'D' around non-transparent pixels
	for (let r = 0; r < sprite.length; r++) {
		for (let c = 0; c < sprite[r]!.length; c++) {
			if (sprite[r]![c] === '.') continue;
			for (let dr = -1; dr <= 1; dr++) {
				for (let dc = -1; dc <= 1; dc++) {
					const row = y + r + dr, col = x + c + dc;
					if (row >= 0 && row < h && col >= 0 && col < w && grid[row]![col] === '.') {
						grid[row]![col] = 'D';
					}
				}
			}
		}
	}
	// Pass 2: sprite on top
	stamp(grid, sprite, x, y);
}

function renderGrid(grid: string[][], bgRgb?: [number,number,number][][]): string {
	const rows: string[] = [];
	// Process pairs of rows for half-block rendering
	for (let y = 0; y < grid.length; y += 2) {
		const topRow = grid[y]!;
		const bottomRow = grid[y + 1] || Array(topRow.length).fill('.');
		let line = '';
		for (let x = 0; x < topRow.length; x++) {
			const top = topRow[x]!;
			const bottom = bottomRow[x]!;
			const topColor = top === '.' && bgRgb ? bgRgb[y]![x]! : (COLOR_RGB[top] ?? [75,135,220]);
			const bottomColor = bottom === '.' && bgRgb ? bgRgb[y+1]?.[x] ?? [75,135,220] : (COLOR_RGB[bottom] ?? [75,135,220]);
			line += halfBlock(topColor, bottomColor);
		}
		rows.push(line);
	}
	return rows.join('\n');
}

// ── Pipe AABB ────────────────────────────────────────────────────────────────

function pipeTop(pipe: PipeDef) { return GROUND_Y - pipe.h - PIPE_CAP_H; }

function pipeAABB(pipe: PipeDef) {
	return { left: pipe.x, right: pipe.x + PIPE_W, top: pipeTop(pipe), bottom: GROUND_Y };
}

function logTop(log: LogDef) { return GROUND_Y - log.h - LOG_CAP_H; }

function logAABB(log: LogDef) {
	return { left: log.x, right: log.x + LOG_W, top: logTop(log), bottom: GROUND_Y };
}

function marioBox(wx: number, y: number) {
	return { left: wx + 4, right: wx + 20, top: y + 2, bottom: y + MARIO_H };
}

function overlaps(a: ReturnType<typeof marioBox>, b: ReturnType<typeof pipeAABB>) {
	return a.right > b.left && a.left < b.right && a.bottom > b.top && a.top < b.bottom;
}

// ── HUD ──────────────────────────────────────────────────────────────────────

const HUD: React.FC<{ coins: number; time: number; coconuts: number }> = ({ coins, time, coconuts }) => (
	<Box flexDirection="row" justifyContent="space-between" width="100%">
		<Text bold color="white"> PURA VIDA          </Text>
		<Text bold color="white">COSTA RICA</Text>
		<Text bold color="white">    TIME</Text>
	</Box>
);

const HUD2: React.FC<{ coins: number; time: number; coconuts: number }> = ({ coins, time, coconuts }) => (
	<Box flexDirection="row" justifyContent="space-between" width="100%">
		<Text bold color="yellow"> ☕×{String(coins).padStart(2, '0')}  🥥×{String(coconuts).padStart(2, '0')}</Text>
		<Text color="gray">← → ↓ space enter t</Text>
		<Text bold color="white">     {String(time).padStart(3, '0')} </Text>
	</Box>
);

// ── Scene ────────────────────────────────────────────────────────────────────

const Scene = () => {
	const { exit } = useApp();
	const { stdout } = useStdout();
	const frame = useFrames(60);
	const [started, setStarted] = useState(false);

	// Viewport adapts to terminal but game world is fixed
	const [vp, setVp] = useState({
		w: Math.max(40, Math.floor((stdout.columns || 80) * 0.9)),
		h: Math.max(20, ((stdout.rows || 24) - 5) * 2),
	});

	useEffect(() => {
		const onResize = () => setVp({
			w: Math.max(40, Math.floor((stdout.columns || 80) * 0.9)),
			h: Math.max(20, ((stdout.rows || 24) - 5) * 2),
		});
		stdout.on('resize', onResize);
		return () => { stdout.off('resize', onResize); };
	}, [stdout]);

	const s = useRef({
		wx: 5, y: FLOOR_Y, vy: 0, vx: 0,
		jumping: false, crouching: false, facing: 'right' as 'left' | 'right', lastMoveFrame: -100,
		coinY: -100, coinWx: 0, showCoin: false, coins: 0, coconutsLeft: 10, timer: 400, lastTimerFrame: 0,
		debris: [] as { x: number; y: number; vx: number; vy: number }[],
		goombas: INITIAL_GOOMBAS.map(g => ({ ...g })),
		dead: false, deadFrame: 0, deadVy: 0, deathWx: 0, lives: 5, deathScreen: false, deathScreenFrame: 0,
		flagSliding: false, flagY: 0, flagScore: 0, ended: false,
		// Corgi companion
		corgiX: -10, corgiY: FLOOR_Y + (MARIO_H - CORGI_H), corgiFacing: 'right' as 'left' | 'right',
		// Macaws
		macaws: [] as { x: number; y: number; vx: number }[],
		lastMacaw: 0,
		// Secret level
		secret: false, fire: true, fireballs: [] as Fireball[],
		coconuts: [] as { x: number; y: number; vx: number; vy: number; life: number }[],
		bowserHp: 10, bowserX: SECRET_W - 30, bowserY: SECRET_GROUND_Y - BOWSER_H,
		bowserVx: -0.5, bowserFires: [] as BowserFire[], bowserFireTimer: 0,
		secretWin: false, secretWinFrame: 0, enterPipeX: 0,
		// Fetch animation
		fetchState: 'none' as 'none' | 'thrown' | 'fetching' | 'returning',
		fetchBallX: 0, fetchBallY: 0, fetchBallVy: 0, fetchLandX: 0, fetchBounces: 0,
	}).current;

	useInput((_input, key) => {
		if (_input === 'q') exit();
		if (!started) { setStarted(true); return; }
		if (s.dead) return;
		if (_input === ' ' && !s.jumping) {
			s.vy = JUMP_VEL;
			s.jumping = true;
		}
		if (key.leftArrow) { s.vx = -MOVE_SPEED; s.facing = 'left'; s.lastMoveFrame = frame; s.crouching = false; }
		if (key.rightArrow) { s.vx = MOVE_SPEED; s.facing = 'right'; s.lastMoveFrame = frame; s.crouching = false; }
		if (key.downArrow && !s.jumping) {
			s.crouching = true; s.vx = 0;
			// Check pipe entry
			if (!s.secret) {
				const mb = marioBox(s.wx, s.y);
				for (let i = 0; i < PIPES.length; i++) {
					if (!SECRET_PIPES.includes(i)) continue;
					const pipe = PIPES[i]!;
					const pb = pipeAABB(pipe);
					if (mb.left >= pb.left && mb.right <= pb.right && mb.bottom >= pb.top - 2 && mb.bottom <= pb.top + 4) {
						s.secret = true; s.fire = true;
						s.enterPipeX = s.wx;
						s.wx = 5; s.y = SECRET_GROUND_Y - MARIO_H; s.vy = 0; s.vx = 0;
						s.jumping = false; s.crouching = false;
						s.bowserHp = 10; s.bowserX = SECRET_W - 30;
						s.bowserY = SECRET_GROUND_Y - BOWSER_H;
						s.bowserVx = -0.5; s.bowserFires = []; s.fireballs = [];
						s.bowserFireTimer = 0; s.secretWin = false;
						break;
					}
				}
			}
		}
		if (key.upArrow || key.leftArrow || key.rightArrow) s.crouching = false;
		// Fireball — Enter key (return)
		if (key.return && s.fire && s.secret && !s.dead && s.fireballs.length < 3) {
			s.fireballs.push({
				x: s.wx + (s.facing === 'right' ? SPRITE_W : -4),
				y: s.y + 8,
				vx: s.facing === 'right' ? 4 : -4,
				life: 40,
			});
		}
		// Coconut — Enter key in overworld
		if (key.return && !s.secret && !s.dead && s.coconuts.length < 3 && s.coconutsLeft > 0) {
			s.coconutsLeft--;
			s.coconuts.push({
				x: s.wx + (s.facing === 'right' ? SPRITE_W : -6),
				y: s.y + 10,
				vx: s.facing === 'right' ? 3.5 : -3.5,
				vy: -2,
				life: 60,
			});
		}
		// Fetch — T key in overworld
		if (_input === 't' && !s.secret && !s.dead && s.fetchState === 'none') {
			s.fetchState = 'thrown';
			s.fetchBallX = s.wx + (s.facing === 'right' ? SPRITE_W : -6);
			s.fetchBallY = s.y + 10;
			s.fetchBallVy = -3;
			s.fetchLandX = Math.max(0, Math.min(WORLD_W - 10, s.wx + (s.facing === 'right' ? 60 : -60)));
			s.fetchBounces = 0;
		}
	});

	// ── Intro screen ──────────────────────────────────────────────────────
	if (!started) {
		return (
			<Box flexDirection="column" alignItems="center" justifyContent="center" height={vp.h / 2}>
				<Box flexDirection="column" alignItems="center" padding={2} borderStyle="double" borderColor="green">
					<Text bold color="yellow">☀  PURA VIDA  ☀</Text>
					<Text bold color="green">The Coffee Trail</Text>
					<Text> </Text>
					<Text>The family finca is in trouble.</Text>
					<Text>Coffee prices have dropped and the farm</Text>
					<Text>needs your help to survive.</Text>
					<Text> </Text>
					<Text>Collect coffee beans across Costa Rica</Text>
					<Text>from the beaches to the cloud forest.</Text>
					<Text>Your loyal corgi will follow you!</Text>
					<Text> </Text>
					<Text bold color="yellow">← → move   SPACE jump   ENTER coconut   T fetch   Q quit</Text>
					<Text> </Text>
					<Text bold color="green">Press any key to start...</Text>
				</Box>
			</Box>
		);
	}

	// Death animation — pop up then fall off screen
	if (s.dead && !s.deathScreen) {
		s.deadVy += GRAVITY;
		s.y += s.deadVy;
		if (frame - s.deadFrame > 90) {
			s.deathScreen = true;
			s.deathScreenFrame = frame;
			s.lives--;
		}
	}

	// Death screen — black with lives count
	if (s.deathScreen) {
		if (frame - s.deathScreenFrame > 120) {
			// Reset and resume — restore camera position before resetting wx
			s.dead = false; s.deathScreen = false;
			s.vy = 0; s.vx = 0; s.jumping = false;
			if (s.secret) {
				s.wx = 5; s.y = SECRET_GROUND_Y - MARIO_H;
				s.bowserFires = []; s.fireballs = [];
			} else {
				s.wx = 5; s.y = FLOOR_Y;
				s.goombas = INITIAL_GOOMBAS.map(g => ({ ...g }));
				s.coconuts = []; s.coconutsLeft = 10;
				s.fetchState = 'none';
			}
		} else {
			return (
				<Box flexDirection="column" alignItems="center" justifyContent="center" height={vp.h / 2}>
					<Box flexDirection="column" alignItems="center" padding={2}>
						<Text bold color="white">{s.secret ? 'SECRET LEVEL' : 'WORLD 1-1'}</Text>
						<Text> </Text>
						<Text bold color="white">{'♥'.repeat(s.lives)}{'♡'.repeat(Math.max(0, 5 - s.lives))}</Text>
						<Text> </Text>
						<Text bold color="white">× {s.lives}</Text>
					</Box>
				</Box>
			);
		}
	}

	if (s.lives <= 0 && !s.ended) {
		return (
			<Box flexDirection="column" alignItems="center" justifyContent="center" height={vp.h / 2}>
				<Box flexDirection="column" alignItems="center" padding={2} borderStyle="double" borderColor="red">
					<Text bold color="red">GAME OVER</Text>
					<Text> </Text>
					<Text bold color="yellow">Coins: {s.coins}</Text>
					<Text> </Text>
					<Text dimColor>Press q to quit</Text>
				</Box>
			</Box>
		);
	}

	// ── SECRET LEVEL ─────────────────────────────────────────────────────────
	if (s.secret) {
		// Secret level win screen
		if (s.secretWin) {
			if (frame - s.secretWinFrame > 180) {
				s.secret = false; s.wx = s.enterPipeX; s.y = FLOOR_Y;
				s.vy = 0; s.vx = 0; s.jumping = false;
			} else {
				return (
					<Box flexDirection="column" alignItems="center" justifyContent="center" height={vp.h / 2}>
						<Box flexDirection="column" alignItems="center" padding={2} borderStyle="double" borderColor="green">
							<Text bold color="green">★ BOWSER DEFEATED! ★</Text>
							<Text> </Text>
							<Text bold color="yellow">+5000 pts</Text>
						</Box>
					</Box>
				);
			}
			return null;
		}

		// Movement in secret level
		if (!s.dead && s.vx !== 0) {
			s.wx = Math.max(0, Math.min(SECRET_W - SPRITE_W, s.wx + s.vx));
			if (frame - s.lastMoveFrame > (s.jumping ? 10 : 3)) s.vx = 0;
		}

		// Physics
		if (!s.dead && (s.jumping || s.y < SECRET_GROUND_Y - MARIO_H)) {
			s.y += s.vy; s.vy += GRAVITY;
			s.jumping = true;
			if (s.y >= SECRET_GROUND_Y - MARIO_H) {
				s.y = SECRET_GROUND_Y - MARIO_H; s.vy = 0; s.jumping = false;
			}
		}

		// Fireballs
		for (const fb of s.fireballs) {
			fb.x += fb.vx; fb.life--;
			// Hit Bowser
			if (s.bowserHp > 0 && fb.x + 4 > s.bowserX && fb.x < s.bowserX + BOWSER_W &&
				fb.y + 4 > s.bowserY && fb.y < s.bowserY + BOWSER_H) {
				s.bowserHp--; fb.life = 0;
				if (s.bowserHp <= 0) { s.secretWin = true; s.secretWinFrame = frame; }
			}
		}
		s.fireballs = s.fireballs.filter(fb => fb.life > 0 && fb.x > -10 && fb.x < SECRET_W + 10);

		// Bowser AI
		if (s.bowserHp > 0) {
			s.bowserX += s.bowserVx;
			if (s.bowserX <= SECRET_W / 2 || s.bowserX >= SECRET_W - BOWSER_W) s.bowserVx = -s.bowserVx;
			// Fire breath
			s.bowserFireTimer++;
			if (s.bowserFireTimer >= 90) {
				s.bowserFireTimer = 0;
				s.bowserFires.push({
					x: s.bowserX - 8, y: s.bowserY + 8, vx: -2.5, life: 50,
				});
			}
		}
		for (const bf of s.bowserFires) {
			bf.x += bf.vx; bf.life--;
			// Hit Mario
			if (!s.dead && bf.x < s.wx + SPRITE_W && bf.x + 8 > s.wx && bf.y < s.y + MARIO_H && bf.y + 3 > s.y) {
				s.dead = true; s.deadFrame = frame; s.deadVy = JUMP_VEL; s.deathWx = s.wx; s.vx = 0;
			}
		}
		s.bowserFires = s.bowserFires.filter(bf => bf.life > 0);

		// Bowser contact kills Mario
		if (!s.dead && s.bowserHp > 0) {
			const mb = marioBox(s.wx, s.y);
			if (mb.right > s.bowserX + 2 && mb.left < s.bowserX + BOWSER_W - 2 &&
				mb.bottom > s.bowserY + 2 && mb.top < s.bowserY + BOWSER_H) {
				s.dead = true; s.deadFrame = frame; s.deadVy = JUMP_VEL; s.deathWx = s.wx; s.vx = 0;
			}
		}

		// Sprite
		const walking = !s.jumping && (frame - s.lastMoveFrame) < 10;
		let sprite: string[];
		if (s.dead) sprite = FM_JUMP;
		else if (s.jumping) sprite = FM_JUMP;
		else if (s.crouching) sprite = FM_CROUCH;
		else if (walking) sprite = [FM_WALK1, FM_WALK2, FM_WALK3][Math.floor(frame / 8) % 3]!;
		else sprite = FM_IDLE;
		if (s.facing === 'right') sprite = sprite.map(r => [...r].reverse().join(''));

		// Render underground
		const grid = Array.from({ length: SECRET_H }, () => Array(vp.w).fill('K') as string[]);
		// Floor
		for (let y = SECRET_GROUND_Y; y < SECRET_H; y++)
			for (let x = 0; x < vp.w; x++) grid[y]![x] = (x + y) % 3 === 0 ? 'o' : 'O';
		// Ceiling bricks
		for (let x = 0; x < vp.w; x += 10) stamp(grid, BRICK, x, 0);

		const camX = Math.max(0, Math.min(SECRET_W - vp.w, Math.round(s.wx - vp.w / 3)));
		// Bowser
		if (s.bowserHp > 0) {
			const bx = Math.round(s.bowserX) - camX;
			const bSprite = (Math.floor(frame / 12) % 2 === 0) ? BOWSER_1 : BOWSER_2;
			stamp(grid, bSprite, bx, s.bowserY);
		}
		// Bowser fires
		for (const bf of s.bowserFires) {
			const bfx = Math.round(bf.x) - camX;
			stamp(grid, BOWSER_FIRE[Math.floor(frame / 6) % 2]!, bfx, Math.round(bf.y));
		}
		// Fireballs
		for (const fb of s.fireballs) {
			const fbx = Math.round(fb.x) - camX;
			stamp(grid, FIREBALL[Math.floor(frame / 4) % 2]!, fbx, Math.round(fb.y));
		}
		// Mario
		stamp(grid, sprite, s.wx - camX, Math.round(s.y));

		const sliceTop = Math.max(0, SECRET_H - vp.h);
		const visible = grid.slice(sliceTop, sliceTop + vp.h);

		const hpBar = s.bowserHp > 0 ? '♥'.repeat(s.bowserHp) + '♡'.repeat(10 - s.bowserHp) : 'DEFEATED';
		return (
			<Box flexDirection="column">
				<Box flexDirection="row" justifyContent="space-between" width="100%">
					<Text bold color="white"> ★ FIRE MARIO ★</Text>
					<Text bold color="red">BOWSER {hpBar}</Text>
				</Box>
				<Box flexDirection="row" justifyContent="space-between" width="100%">
					<Text bold color="yellow"> ☕×{String(s.coins).padStart(2, '0')}  Enter=🔥</Text>
					<Text bold color="white">{'♥'.repeat(s.lives)}{'♡'.repeat(Math.max(0, 5 - s.lives))}</Text>
				</Box>
				<Box borderStyle="round" borderColor="red">
					<Text>{renderGrid(visible)}</Text>
				</Box>
			</Box>
		);
	}

	// ── OVERWORLD ────────────────────────────────────────────────────────────

	// Apply horizontal movement each frame (vx decays so Mario stops after key release)
	if (!s.dead && s.vx !== 0) {
		const nx = s.wx + s.vx;
		const proposed = marioBox(nx, s.y);
		let blocked = false;
		for (const pipe of PIPES) {
			const pb = pipeAABB(pipe);
			if (overlaps(proposed, pb)) {
				if (proposed.bottom <= pb.top + 3) continue;
				blocked = true;
				break;
			}
		}
		for (const log of LOGS) {
			if (blocked) break;
			const lb = logAABB(log);
			if (overlaps(proposed, lb)) {
				if (proposed.bottom <= lb.top + 3) continue;
				blocked = true;
			}
		}
		if (!blocked) s.wx = Math.max(0, Math.min(WORLD_W - SPRITE_W, nx));
		// Decay: keep momentum while jumping, stop quickly on ground
		if (frame - s.lastMoveFrame > (s.jumping ? 10 : 3)) s.vx = 0;
	}

	// Camera — horizontal scroll, vertical tracks Mario keeping ground visible
	const camX = Math.max(0, Math.min(WORLD_W - vp.w, Math.round(s.wx - vp.w / 3)));
	// Vertical camera: anchor ground at bottom, clamp viewport to game bounds
	const camY = Math.max(0, GAME_H - vp.h);

	// Find floor (ground or pipe/log top)
	let floorY = FLOOR_Y;
	const mb = marioBox(s.wx, s.y);
	for (const pipe of PIPES) {
		const pb = pipeAABB(pipe);
		if (mb.right > pb.left && mb.left < pb.right && mb.bottom <= pb.top + 4) {
			floorY = Math.min(floorY, pb.top - MARIO_H);
		}
	}
	for (const log of LOGS) {
		const lb = logAABB(log);
		if (mb.right > lb.left && mb.left < lb.right && mb.bottom <= lb.top + 4) {
			floorY = Math.min(floorY, lb.top - MARIO_H);
		}
	}

	// Physics
	if (!s.dead && (s.jumping || s.y < floorY)) {
		s.y += s.vy;
		s.vy += GRAVITY;
		s.jumping = true;

		// Block collision from below
		for (const blk of BLOCKS) {
			if (blk.hit && blk.type === 'brick') continue;
			const bB = blk.y + BLOCK_H;
			if (s.vy < 0 && s.y <= bB && s.y > blk.y && mb.right > blk.x && mb.left < blk.x + BLOCK_H) {
				if (blk.type === 'question' && !blk.hit) {
					blk.hit = true;
					s.showCoin = true;
					s.coinWx = blk.x;
					s.coinY = blk.y - 8;
					s.coins++;
				} else if (blk.type === 'brick' && !blk.hit) {
					blk.hit = true;
					const cx = blk.x + 4, cy = blk.y;
					s.debris.push(
						{ x: cx - 3, y: cy, vx: -1.5, vy: -4 },
						{ x: cx + 3, y: cy, vx: 1.5, vy: -4 },
						{ x: cx - 2, y: cy + 3, vx: -1, vy: -2.5 },
						{ x: cx + 2, y: cy + 3, vx: 1, vy: -2.5 },
					);
				}
				s.vy = 1;
			}
		}

		// Re-check floor (may have moved horizontally)
		floorY = FLOOR_Y;
		const mb2 = marioBox(s.wx, s.y);
		for (const pipe of PIPES) {
			const pb = pipeAABB(pipe);
			if (mb2.right > pb.left && mb2.left < pb.right && s.y + MARIO_H <= pb.top + 5) {
				floorY = Math.min(floorY, pb.top - MARIO_H);
			}
		}
		for (const log of LOGS) {
			const lb = logAABB(log);
			if (mb2.right > lb.left && mb2.left < lb.right && s.y + MARIO_H <= lb.top + 5) {
				floorY = Math.min(floorY, lb.top - MARIO_H);
			}
		}

		if (s.y >= floorY) {
			s.y = floorY;
			s.vy = 0;
			s.jumping = false;
		}
	}

	if (s.showCoin) {
		s.coinY -= 0.3;
		if (s.coinY < -10) s.showCoin = false;
	}

	// Goomba AI — patrol and stomp detection
	for (const g of s.goombas) {
		if (g.dead > 0) { g.dead++; continue; }
		g.x += g.vx;
		// Reverse at pipes
		for (const pipe of PIPES) {
			if (g.x + GOOMBA_W > pipe.x && g.x < pipe.x + PIPE_W) g.vx = -g.vx;
		}
		// Reverse at logs
		for (const log of LOGS) {
			if (g.x + GOOMBA_W > log.x && g.x < log.x + LOG_W) g.vx = -g.vx;
		}
		// Reverse at world edges
		if (g.x <= 0 || g.x >= WORLD_W - GOOMBA_W) g.vx = -g.vx;
		// Stomp: Mario falling onto Goomba
		const mx = s.wx, my = Math.round(s.y);
		if (s.vy > 0 && mx + SPRITE_W > g.x + 2 && mx < g.x + GOOMBA_W - 2 &&
			my + MARIO_H >= g.y && my + MARIO_H <= g.y + 5) {
			g.dead = 1;
			s.vy = JUMP_VEL * 0.6; // bounce
			s.coins++;
		} else if (!s.dead && mx + SPRITE_W > g.x + 3 && mx < g.x + GOOMBA_W - 3 &&
			my + MARIO_H > g.y + 4 && my < g.y + 18) {
			// Side hit — Mario dies
			s.dead = true; s.deadFrame = frame; s.deadVy = JUMP_VEL;
			s.deathWx = s.wx; s.vx = 0;
		}
	}
	s.goombas = s.goombas.filter(g => g.dead === 0 || g.dead < 30);

	// Coconut physics
	for (const co of s.coconuts) {
		co.x += co.vx; co.vy += 0.3; co.y += co.vy; co.life--;
		if (co.y >= GROUND_Y - 10) { co.y = GROUND_Y - 10; co.vy = -co.vy * 0.5; }
		for (const g of s.goombas) {
			if (g.dead > 0) continue;
			if (co.x + 10 > g.x && co.x < g.x + GOOMBA_W && co.y + 10 > g.y && co.y < g.y + 18) {
				g.dead = 1; co.life = 0; s.coins++;
			}
		}
	}
	s.coconuts = s.coconuts.filter(co => co.life > 0);

	// Fetch ball physics
	if (s.fetchState === 'thrown') {
		const dir = s.fetchLandX > s.fetchBallX ? 1 : -1;
		s.fetchBallX += dir * 2;
		s.fetchBallVy += 0.3;
		s.fetchBallY += s.fetchBallVy;
		if (s.fetchBallY >= GROUND_Y - 10) {
			s.fetchBallY = GROUND_Y - 10;
			s.fetchBounces++;
			if (s.fetchBounces >= 3) {
				s.fetchBallX = s.fetchLandX;
				s.fetchState = 'fetching';
			} else {
				s.fetchBallVy *= -0.4;
			}
		}
	}

	// Timer countdown (~1 per second at 60fps)
	if (frame - s.lastTimerFrame >= 60 && s.timer > 0) {
		s.timer--;
		s.lastTimerFrame = frame;
	}

	// Corgi follows player with lag (overridden during fetch)
	let corgiTarget: number;
	let corgiSpeed = 0.08;
	if (s.fetchState === 'fetching') {
		corgiTarget = s.fetchBallX;
		corgiSpeed = 0.15;
	} else if (s.fetchState === 'returning') {
		corgiTarget = s.wx - 20;
		corgiSpeed = 0.15;
	} else {
		corgiTarget = s.wx - 20;
	}
	const dx = corgiTarget - s.corgiX;
	if (Math.abs(dx) > 1) {
		s.corgiX += dx * corgiSpeed;
		s.corgiFacing = dx > 0 ? 'right' : 'left';
	}
	// Fetch state transitions
	if (s.fetchState === 'fetching' && Math.abs(s.corgiX - s.fetchBallX) < 5) {
		s.fetchState = 'returning';
	} else if (s.fetchState === 'returning' && Math.abs(s.corgiX - (s.wx - 20)) < 5) {
		s.fetchState = 'none';
	}
	s.corgiY = FLOOR_Y + (MARIO_H - CORGI_H);

	// Macaw fly-bys: spawn every 600 frames (~10s), fly across screen
	if (frame - s.lastMacaw >= 600) {
		s.lastMacaw = frame;
		const fromLeft = Math.random() > 0.5;
		s.macaws.push({
			x: fromLeft ? camX - MACAW_W : camX + vp.w + MACAW_W,
			y: 2 + Math.floor(Math.random() * 10),
			vx: fromLeft ? 3 : -3,
		});
	}
	for (const m of s.macaws) m.x += m.vx;
	s.macaws = s.macaws.filter(m => m.x > camX - 40 && m.x < camX + vp.w + 40);

	// Sprite
	const walking = !s.jumping && (frame - s.lastMoveFrame) < 10;
	const idleTime = frame - s.lastMoveFrame;
	const walkPhase = Math.floor(frame / 6) % 4; // 0,1,2,3
	let sprite: string[];
	let walkBob = 0;
	if (s.dead) sprite = M_JUMP;
	else if (s.jumping) sprite = M_JUMP;
	else if (s.crouching) sprite = M_CROUCH;
	else if (walking) {
		sprite = (walkPhase < 2) ? M_WALK1 : M_WALK2;
		walkBob = (walkPhase === 1 || walkPhase === 3) ? -1 : 0;
	}
	else if (idleTime > 300 && Math.floor(frame / 90) % 2 === 1) sprite = CR_THINKING;
	else sprite = M_IDLE;
	if (s.facing === 'right') sprite = sprite.map(r => [...r].reverse().join(''));

	// ── Render: build full game grid, then extract viewport slice ─────────

	const grid = Array.from({ length: GAME_H }, () => Array(vp.w).fill('.') as string[]);

	// Background: 3-tile panorama (BG_RGB=tile1, BG_RGB=tile2 mirror, BG_RGB3=tile3), wraps via tile3
	const bgRgb: [number,number,number][][] = [];
	const bgScroll = Math.floor(camX * BG_PARALLAX);
	for (let y = 0; y < GAME_H; y++) {
		const row: [number,number,number][] = [];
		for (let x = 0; x < vp.w; x++) {
			const wx = x + bgScroll;
			const tile = ((wx / BG_TILE_W | 0) % 3 + 3) % 3;
			const bx = ((wx % BG_TILE_W) + BG_TILE_W) % BG_TILE_W;
			row.push(tile < 2 ? BG_RGB[y]![bx]! : BG_RGB3[y]![bx]!);
		}
		bgRgb.push(row);
	}

	// Pipes
	for (const pipe of PIPES) {
		const px = pipe.x - camX;
		const pt = pipeTop(pipe);
		stamp(grid, PIPE_CAP, px, pt);
		for (let i = 0; i < pipe.h; i++) stamp(grid, PIPE_BODY, px, pt + PIPE_CAP_H + i);
	}

	// Logs
	for (const log of LOGS) {
		const lx = log.x - camX;
		const lt = logTop(log);
		stampOutlined(grid, LOG_CAP, lx, lt);
		for (let i = 0; i < log.h; i++) stampOutlined(grid, LOG_BODY, lx, lt + LOG_CAP_H + i);
	}

	// Flag pole at end of world
	const FLAG_X = 1170;
	const fx = FLAG_X - camX;
	const flagPoleH = 72;  // tall flagpole
	const flagTop = GROUND_Y - flagPoleH;

	// Detect Mario touching the pole
	if (!s.flagSliding && !s.ended && s.wx + 13 >= FLAG_X && s.wx < FLAG_X + 5) {
		s.flagSliding = true;
		// Score based on height: higher = more points
		const heightAboveGround = GROUND_Y - (s.y + MARIO_H);
		s.flagScore = Math.max(100, Math.min(5000, Math.round(heightAboveGround * 200)));
		s.flagY = flagTop + 1;
		s.vx = 0;
	}

	// Flag slides down
	if (s.flagSliding && !s.ended) {
		s.flagY = Math.min(s.flagY + 0.5, GROUND_Y - 7);
		if (s.flagY >= GROUND_Y - 7) s.ended = true;
	}

	// Render pole
	stamp(grid, FLAG_BALL, fx, flagTop - 3);
	const flagFrame = FLAG_FRAMES[Math.floor(frame / 8) % 6]!;
	const flagDrawY = s.flagSliding ? Math.round(s.flagY) : flagTop + 1;
	stamp(grid, flagFrame, fx + 1, flagDrawY);
	for (let i = 0; i < flagPoleH; i++) stamp(grid, FLAG_POLE_LINE, fx + 1, flagTop + i);
	stamp(grid, FLAG_BASE, fx - 1, GROUND_Y - 2);

	// Blocks
	for (const blk of BLOCKS) {
		const bx = blk.x - camX;
		if (bx < -10 || bx > vp.w) continue;
		if (blk.type === 'brick') {
			if (!blk.hit) stamp(grid, BRICK, bx, blk.y);
		} else {
			stamp(grid, blk.hit ? Q_HIT : Q_BLOCK, bx, blk.y);
		}
	}

	// Coin
	if (s.showCoin) stamp(grid, COIN_RAW, s.coinWx - camX + 2, Math.round(s.coinY));

	// Debris
	for (const d of s.debris) {
		d.x += d.vx; d.y += d.vy; d.vy += 0.3;
		const dx = Math.round(d.x) - camX, dy = Math.round(d.y);
		if (dy >= 0 && dy < GAME_H && dx >= 0 && dx < vp.w) {
			grid[dy]![dx] = 'O';
			if (dx + 1 < vp.w) grid[dy]![dx + 1] = 'O';
		}
	}
	s.debris = s.debris.filter(d => d.y < GAME_H);

	// Goombas
	for (const g of s.goombas) {
		const gx = Math.round(g.x) - camX;
		if (gx < -GOOMBA_W || gx > vp.w) continue;
		if (g.dead > 0) stampOutlined(grid, GOOMBA_FLAT, gx, g.y);
		else {
			let gs = (Math.floor(frame / 8) % 2 === 0) ? GOOMBA_1 : GOOMBA_2;
			if (g.vx < 0) gs = gs.map(r => [...r].reverse().join(''));
			stampOutlined(grid, gs, gx, g.y);
		}
	}

	// Coconuts
	for (const co of s.coconuts) {
		const cox = Math.round(co.x) - camX;
		if (cox > -6 && cox < vp.w) stamp(grid, COCONUT[Math.floor(frame / 4) % 2]!, cox, Math.round(co.y));
	}

	// Fetch ball
	if (s.fetchState === 'thrown' || s.fetchState === 'fetching') {
		const fbx = Math.round(s.fetchBallX) - camX;
		if (fbx > -10 && fbx < vp.w) stamp(grid, FETCH_BALL[Math.floor(frame / 4) % 2]!, fbx, Math.round(s.fetchBallY));
	}

	// Macaws
	for (const m of s.macaws) {
		const mSprite = MACAW_FRAMES[Math.floor(frame / 6) % 4]!;
		const flipped = m.vx < 0 ? mSprite.map(r => [...r].reverse().join('')) : mSprite;
		stamp(grid, flipped, Math.round(m.x) - camX, m.y);
	}

	// Corgi
	const corgiWalking = Math.abs(s.wx - 20 - s.corgiX) > 2;
	let corgiSprite: string[];
	if (s.fetchState === 'fetching' || s.fetchState === 'returning') {
		corgiSprite = [CORGI_WALK1, CORGI_WALK2][Math.floor(frame / 6) % 2]!;
	} else if (corgiWalking) corgiSprite = [CORGI_WALK1, CORGI_WALK2][Math.floor(frame / 8) % 2]!;
	else if (idleTime > 600) corgiSprite = CORGI_SLEEP;
	else if (idleTime > 300) corgiSprite = CORGI_PLAY;
	else corgiSprite = CORGI_IDLE;
	if (s.corgiFacing === 'right') corgiSprite = corgiSprite.map(r => [...r].reverse().join(''));
	const cx = Math.round(s.corgiX) - camX, cy = Math.round(s.corgiY);
	stampOutlined(grid, corgiSprite, cx, cy);
	// Zzz bubble when sleeping
	if (idleTime > 600) {
		const zf = Math.floor(frame / 20) % 3;
		const ZZZ = [['..z'], ['.z.','..Z'], ['z..', '.Z.', '..Z']];
		const zy = cy + 8 - ZZZ[zf]!.length;
		stamp(grid, ZZZ[zf]!, cx + 10, zy);
	}

	// Mario
	stampOutlined(grid, sprite, s.wx - camX, Math.round(s.y) + walkBob);

	// Extract visible vertical slice
	const sliceTop = camY;
	const sliceBot = Math.min(GAME_H, camY + vp.h);
	const visible = grid.slice(sliceTop, sliceBot);
	const visibleBg = bgRgb.slice(sliceTop, sliceBot);

	if (s.ended) {
		return (
			<Box flexDirection="column" alignItems="center" justifyContent="center" width="100%" height={vp.h / 2}>
				<Box flexDirection="column" alignItems="center" padding={2} borderStyle="double" borderColor="yellow">
					<Text bold color="yellow">☀  ¡PURA VIDA!  ☀</Text>
					<Text> </Text>
					<Text bold color="green">The finca is saved!</Text>
					<Text>Your coffee beans will keep the farm alive.</Text>
					<Text> </Text>
					<Text bold color="white">☕ Coffee beans: {s.coins}</Text>
					<Text bold color="white">🥥 Coconuts used: {10 - s.coconutsLeft}</Text>
					<Text bold color="white">⏱  Time bonus: {s.timer * 50}</Text>
					<Text bold color="white">🏁 Flag bonus: {s.flagScore}</Text>
					<Text> </Text>
					<Text bold color="yellow">TOTAL: {s.flagScore + s.coins * 200 + s.timer * 50}</Text>
					<Text> </Text>
					<Text dimColor>Press q to quit — ¡Gracias por jugar!</Text>
				</Box>
			</Box>
		);
	}

	return (
		<Box flexDirection="column">
			<HUD coins={s.coins} time={s.timer} coconuts={s.coconutsLeft} />
			<HUD2 coins={s.coins} time={s.timer} coconuts={s.coconutsLeft} />
			<Box borderStyle="round" borderColor="blue">
				<Text>{renderGrid(visible, visibleBg)}</Text>
			</Box>
			{s.flagSliding && <Text bold color="yellow"> +{s.flagScore} pts!</Text>}
		</Box>
	);
};

render(<Scene />);
