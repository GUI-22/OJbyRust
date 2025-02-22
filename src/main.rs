use actix_web::{get, middleware::Logger, post, web, App, HttpServer, Responder, put};
use actix_web::{HttpResponse, HttpRequest};
use env_logger;
use log;
use serde_json::Value;
use serde_derive::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;
use std::process::Command;
use std::fs::File;
use std::process::Stdio;
use chrono::{Utc, DateTime, NaiveDateTime};
use std::time::Instant;
use wait_timeout::ChildExt;
use std::time::Duration;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use jsonwebtoken::{encode, decode, Algorithm, EncodingKey, DecodingKey, Header, Validation};
use rand::RngCore;
//声明结构体和变量
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Server {
    bind_address: Option<String>,
    bind_port: Option<u16>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Case {
    time_limit: u128,
    memory_limit: i32,
    score: f64,
    input_file: String,
    answer_file: String
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Misc {
    packing: Option<Vec<Vec<usize>>>,
    special_judge: Option<Vec<String>>,
    dynamic_ranking_ratio: Option<f64>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Problem {
    id: i32,
    name: String,
    #[serde(rename = "type")]
    ty: String,
    misc: Misc,
    cases: Vec<Case>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Language {
    name: String,
    file_name: String,
    command: Vec<String>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Setting {
    server: Server,
    problems: Vec<Problem>,
    languages: Vec<Language>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Error {
    code: i32,
    reason: String,
    message: String
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct PostJob {
    source_code: String,
    language: String,
    user_id: i32,
    contest_id: i32,
    problem_id: i32
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct CaseReturn {
    id: i32,
    result: String,
    time: u128,
    memory: i32,
    info: String
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct JsonResponse {
    id: i32,
    created_time: String,
    updated_time: String,
    submission: PostJob,
    state: String,
    result: String,
    score: f64,
    cases: Vec<CaseReturn>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct User {
    id: Option<i32>,
    name: String
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Performance {
    if_did: bool,
    submission_time: String,
    score: f64,
    submission_count: i32,
    job_id: i32
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserInContest {
    user_info: User,
    performances: HashMap<i32, Performance>,
    total_score: f64,
    submssion_time: String,
    total_submission_count: i32,
    rank: i32
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserInContestJson {
    user: User,
    rank: i32,
    scores: Vec<f64>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Contest {
    id: Option<i32>,
    name: String,
    from: String,
    to: String,
    problem_ids: Vec<i32>,
    user_ids: Vec<i32>,
    submission_limit: i32
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProbInfo {
    min_time: Vec<u128>,
    ratio: Option<f64>,
    full_score: Vec<f64>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserPlus {
    id: Option<i32>,
    name: String,
    key: String,
    identity: Option<String>
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserPlusClaim {
    id: Option<i32>,
    name: String,
    identity: Option<String>,
    exp: usize
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChangeName {
    before: Option<String>,
    after: String
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Info {
    job_list: Vec<JsonResponse>,
    user_ist: Vec<User>,
    contest_list: Vec<Contest>
}
//创建全局变量
lazy_static! {
    static ref JOB_LIST: Arc<Mutex<Vec<JsonResponse>>> = Arc::new(Mutex::new(Vec::new()));
    static ref USERS: Arc<Mutex<Vec<User>>> = Arc::new(Mutex::new(Vec::new()));
    static ref USER_PLUS_LIST: Arc<Mutex<Vec<UserPlus>>> = Arc::new(Mutex::new(Vec::new()));
    static ref CONTESTS: Arc<Mutex<Vec<Contest>>> = Arc::new(Mutex::new(Vec::new()));
    static ref BLACKLIST: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
}
//API
#[post("/jobs")]
async fn post_jobs(body: web::Json<PostJob>, setting: web::Data<Setting>, 
req: HttpRequest, secret_key: web::Data<DecodingKey>, if_token: web::Data<bool>) -> impl Responder {
    //鉴权
    if *if_token == true.into() {
        let deco_result = decoding(req, secret_key);
        if deco_result == None {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else if deco_result != Some(String::from("CommonUser")) {
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Only CommonUser have the right."),
            });
        }
    }
    let utc_time_create: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
    let mut temp_problem: Problem = Problem { id:0, name: String::new(), ty: String::new(), misc: Misc { packing: None, special_judge: None, dynamic_ranking_ratio: None }, cases: Vec::new() };
    let mut temp_language: Language = Language { name:String::new(), file_name: String::new(), command: vec![] };
    let mut check_lan = 0;
    let mut check_prob_id = 0;
    let mut check_user_id = 0;
    let mut check_contest_id = 1;
    let lock = JOB_LIST.lock().unwrap();
    let job_id = lock.len();
    drop(lock);
    let mut dir_path = PathBuf::new();
    dir_path.push(String::from("target"));
    dir_path.push(format!("tmp_{}", &job_id.to_string()));
    let mut temp_code_file = dir_path.clone();
    //检查
    for i in &setting.languages {
        if i.name == body.language {
            check_lan = 1;
            temp_code_file.push(&i.file_name);
            temp_language = i.clone();
            break;
        }
    }
    for i in &setting.problems {
        if i.id == body.problem_id {
            check_prob_id = 1;
            temp_problem = i.clone();
            break;
        }
    }
    let user_list = USERS.lock().unwrap();
    for i in user_list.iter() {
        if i.id == Some(body.user_id) {
            check_user_id = 1;
        }
    }
    drop(user_list);
    let contest_list = CONTESTS.lock().unwrap();
    if body.contest_id > contest_list.len() as i32 {
        check_contest_id = 0;
    }
    if check_lan == 0 || check_prob_id == 0 || check_user_id == 0 || check_contest_id == 0 {
        let _ = std::fs::remove_dir_all(&dir_path);
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : String::from("HTTP 404 Not Found"),
        });
    }
    //比赛有关的检查
    if body.contest_id > 0 {
        //Inspired from GPT
        //判断字符串是否符合 format 的格式
        let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
        if contest_list[body.contest_id as usize - 1].problem_ids.contains(&body.problem_id) == false ||
        contest_list[body.contest_id as usize - 1].user_ids.contains(&body.user_id) == false ||
        NaiveDateTime::parse_from_str(&utc_time_create.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(), format).unwrap() <
        NaiveDateTime::parse_from_str(&contest_list[body.contest_id as usize- 1].from, format).unwrap() ||
        NaiveDateTime::parse_from_str(&utc_time_create.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(), format).unwrap() >
        NaiveDateTime::parse_from_str(&contest_list[body.contest_id as usize- 1].to, format).unwrap() {
            return HttpResponse::BadRequest().json(Error {
                code : 1,
                reason : String::from("ERR_INVALID_ARGUMENT"), 
                message: String::from("HTTP 400 Bad Request"),
            })
        }
        if contest_list[body.contest_id as usize - 1].submission_limit > 0 {
            let mut sub_count = 0;
            let lock = JOB_LIST.lock().unwrap();
            for job in lock.iter() {
                if job.submission.contest_id == body.contest_id && job.submission.user_id == body.user_id
                && job.submission.problem_id == body.problem_id {
                    sub_count += 1;
                }
            }
            drop(lock);
            if sub_count >= contest_list[body.contest_id as usize - 1].submission_limit {
                return HttpResponse::BadRequest().json(Error {
                    code : 4,
                    reason : String::from("ERR_RATE_LIMIT"), 
                    message: String::from("HTTP 400 Bad Request"),
                })
            }
        }
    }
    drop(contest_list);
    //先构建所有测试点
    let json_response: JsonResponse = JsonResponse {
        id: job_id as i32,
        created_time: utc_time_create.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        updated_time: utc_time_create.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        submission: body.clone(),
        state: String::from("Queueing"),
        result: String::from("Waiting"),
        score: 0.0,
        cases: Vec::new()
    };
    let mut lock = JOB_LIST.lock().unwrap();
    lock.push(json_response);
    for i in 0..temp_problem.cases.len() + 1 {
        let temp_case: CaseReturn = CaseReturn { id: i as i32, result: String::from("Waiting"), 
        time: 0, memory: 0, info: String::from("") };
        lock[job_id].cases.push(temp_case);
    }
    save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");

    drop(lock);
    //进入异步
    actix_web::rt::spawn(async move {
    let block_result = actix_web::web::block(move ||{
    
    //删、建文件夹（忽略“找不到文件夹”的错误）
    let _ = std::fs::remove_dir_all(&dir_path);
    match std::fs::create_dir(&dir_path) {
        Ok(()) => {}
        Err(_err) => { return Err("Internal Error".to_string()); }
    }
    //将源代码写入 temp_code_file
    let mut f: File;
    match std::fs::File::create(temp_code_file.clone()) {
        Ok(temp_f) => {f = temp_f}
        Err(_err) => {
            let _ = std::fs::remove_dir_all(&dir_path);
            return Err("Internal Error".to_string()); 
        }
    }
    match f.write(body.source_code.as_bytes()) {
        Ok(_num) => {}
        Err(_err) => {
            let _ = std::fs::remove_dir_all(&dir_path);
            return Err("Internal Error".to_string());
        }
    }
    //构建编译 command
    let mut command_clone = temp_language.command;
    for j in &mut command_clone {
        if j == "%OUTPUT%" {
            *j = dir_path.clone().to_str().unwrap().to_string();
            (*j).push_str("/test.exe");
            break;
        }
    }
    for j in &mut command_clone {
        if j == "%INPUT%" {
            *j = temp_code_file.clone().to_str().unwrap().to_string();
            break;
        }
    }
    let mut lock = JOB_LIST.lock().unwrap();
    //编译
    lock[job_id].state = String::from("Running");
    lock[job_id].result = String::from("Running");
    lock[job_id].cases[0].result = String::from("Running");
    drop(lock);
    let command_clone_slice = &command_clone[1..];
    let compile_start = Instant::now();
    let status: std::process::ExitStatus;
    match Command::new(command_clone[0].clone()).args(command_clone_slice)
    .status() {
        Ok(temp_status) => {
            status = temp_status;
        }
        Err(_err) => {//无法运行编译器
            let _ = std::fs::remove_dir_all(&dir_path);
            return Err("Internal Error".to_string());
        }
    }
    let compile_duration = compile_start.elapsed();
    
    //编译失败（编译器异常退出）
    if status.success() == false {
        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let mut lock = JOB_LIST.lock().unwrap();
        lock[job_id].updated_time = formatted_time;
        lock[job_id].state = String::from("Finished");
        lock[job_id].result = String::from("Compilation Error");
        lock[job_id].cases[0].result = String::from("Compilation Error");
        lock[job_id].cases[0].time = compile_duration.as_micros();
        let _ = std::fs::remove_dir_all(&dir_path);
        save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
        drop(lock);
        return Ok(());
    } //编译成功，进入循环判断 cases
    else {
        let mut lock = JOB_LIST.lock().unwrap();
        lock[job_id].cases[0].result = String::from("Compilation Success");
        lock[job_id].cases[0].time = compile_duration.as_micros();
        drop(lock);
        let mut id_cnt = 0;
        match temp_problem.misc.packing {
            None => {
                for i in &mut temp_problem.cases {
                    id_cnt += 1;
                    let mut check_right = 1;
                    let in_file: File;
                    match File::open(i.input_file.clone()) {
                        Ok(temp_file) => {
                            in_file = temp_file;
                        }
                        Err(_err) => {
                            let _ = std::fs::remove_dir_all(&dir_path);
                            return Err("Internal Error".to_string());
                        }
                    }
                    let out_file: File;
                    let mut out_file_path = dir_path.clone();
                    out_file_path.push("output.txt");
                    match File::create(out_file_path) {
                        Ok(temp_file) => {
                            out_file = temp_file;
                        }
                        Err(_err) => {
                            let _ = std::fs::remove_dir_all(&dir_path);
                            return Err("Internal Error".to_string());
                        }
                    }
                    //运行该测试点
                    let case_start = Instant::now();
                    let time_limit_deration = Duration::from_micros(((i.time_limit as f64) * 1.05 ) as u64);
                    let status: std::process::ExitStatus;
                    let mut exe_path = dir_path.clone();
                    exe_path.push("test.exe");
                    let mut child = Command::new(&exe_path)
                    .stdin(Stdio::from(in_file))
                    .stdout(Stdio::from(out_file))
                    .stderr(Stdio::null())
                    .spawn().unwrap();
                    //判断超时
                    //Adapted from https://docs.rs/wait-timeout/latest/wait_timeout/ by Rust 官方 on 2023-07-14
                    match child.wait_timeout(time_limit_deration).unwrap() {
                        Some(temp_status) => status = temp_status,
                        None => {
                            let case_duration = case_start.elapsed();
                            let processing_time = case_duration.as_micros();
                            let mut lock = JOB_LIST.lock().unwrap();
                            lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                            drop(lock);
                            child.kill().unwrap();
                            let _ = child.wait().unwrap().code();
                            continue;
                        }
                    };
                    //记录运行用时
                    let case_duration = case_start.elapsed();
                    let processing_time = case_duration.as_micros();
                    //判断退出状态码
                    if let Some(code) = status.code() {
                        if code != 0 {
                            let mut lock = JOB_LIST.lock().unwrap();
                            lock[job_id].cases[id_cnt as usize].result = String::from("Runtime Error");
                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                            //更新时间
                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            lock[job_id].updated_time = formatted_time;
                            save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                            drop(lock);
                            continue;
                        }
                    }
                    //判断超时
                    if processing_time > i.time_limit {
                        let mut lock = JOB_LIST.lock().unwrap();
                        lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                        //更新时间
                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                        lock[job_id].updated_time = formatted_time;
                        drop(lock);
                        continue;
                    }
                    else {
                        //未超时，对比输入输出
                        let mut out_file: File;
                        let mut out_file_path = dir_path.clone();
                        out_file_path.push("output.txt");
                        match File::open(out_file_path.clone()) {
                            Ok(temp_file) => {
                                out_file = temp_file;
                            }
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        let mut ans_file: File;
                        match File::open(String::from(i.answer_file.clone())) {
                            Ok(temp_file) => {
                                ans_file = temp_file;
                            }
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        let mut out_str = String::new();
                        let mut ans_str = String::new();
                        match out_file.read_to_string(&mut out_str) {
                            Ok(_num) => {}
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        match ans_file.read_to_string(&mut ans_str){
                            Ok(_num) => {}
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        if temp_problem.ty == "standard" {
                            let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                            if out_str_split.last().unwrap() == "" {
                                out_str_split.pop();
                            }
                            let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                            if ans_str_split.last().unwrap() == "" {
                                ans_str_split.pop();
                            }
                            if out_str_split.len() != ans_str_split.len() {
                                check_right = 0;
                            } else {
                                for j in 0..out_str_split.len() {
                                    if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                        check_right = 0;
                                        break;
                                    }
                                }
                            }
                            if check_right == 1 {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].score += i.score;
                                drop(lock);
                            } 
                        }
                        else if temp_problem.ty == "strict" {
                            if out_str != ans_str {
                                check_right = 0;
                            }
                            else {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].score += i.score;
                                drop(lock);
                            }
                        }
                        else if temp_problem.ty == "dynamic_ranking" {
                            let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                            if out_str_split.last().unwrap() == "" {
                                out_str_split.pop();
                            }
                            let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                            if ans_str_split.last().unwrap() == "" {
                                ans_str_split.pop();
                            }
                            if out_str_split.len() != ans_str_split.len() {
                                check_right = 0;
                            } else {
                                for j in 0..out_str_split.len() {
                                    if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                        check_right = 0;
                                        break;
                                    }
                                }
                            }
                            if check_right == 1 {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].score += i.score * (1.0 - temp_problem.misc.dynamic_ranking_ratio.unwrap());
                                drop(lock);
                            }
                        }
                        //temp_problem.ty == "spj"
                        else {  
                            let mut spj_out_path = dir_path.clone();
                            let mut spj_out_info: String = String::new();
                            spj_out_path.push("spj_out.txt");
                            let spj_out_file: File;
                            if let Ok(temp_file) = File::create(spj_out_path.clone()) {
                                spj_out_file = temp_file;
                            } else {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                            let mut spj_command = temp_problem.misc.special_judge.clone().unwrap().clone();
                            for str in &mut spj_command {
                                if str == "%OUTPUT%" {
                                    *str = out_file_path.clone().to_str().unwrap().to_string();
                                }
                                else if str == "%ANSWER%" {
                                    *str = i.answer_file.clone();
                                }
                            }
                            let spj_command_slice = &spj_command[1..];
                            match Command::new(spj_command[0].clone()).args(spj_command_slice)
                            .stdout(Stdio::from(spj_out_file)).stderr(Stdio::null()).status() {
                                Ok(temp_status) => {
                                    if temp_status.success() == false || temp_status.code() != Some(0) {
                                        let mut lock = JOB_LIST.lock().unwrap();
                                        lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                        drop(lock);
                                        continue;
                                    }
                                }
                                Err(_err) => {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                    //更新时间
                                    let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                    let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                    lock[job_id].updated_time = formatted_time;
                                    drop(lock);
                                    continue;
                                }
                            }
                            let mut spj_out_file: File;
                            if let Ok(temp_file) = File::open(spj_out_path.clone()) {
                                spj_out_file = temp_file;
                            } else {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string())
                            }
                            match spj_out_file.read_to_string(&mut spj_out_info) {
                                Ok(_num) => {}
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            let mut spj_out_split: Vec<String> = spj_out_info.split('\n').map(|s| s.to_string()).collect();
                            if spj_out_split.last().unwrap() == "" {
                                spj_out_split.pop();
                            }
                            if spj_out_split.len() != 2 {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                //更新时间
                                let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                lock[job_id].updated_time = formatted_time;
                                drop(lock);
                                continue;
                            }
                            else {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].info = spj_out_split[1].clone();
                                if &spj_out_split[0] == "Accepted" {
                                    check_right = 1;
                                    lock[job_id].score += i.score;
                                    
                                } else {
                                    lock[job_id].cases[id_cnt as usize].result = spj_out_split[0].clone();
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                    //更新时间
                                    let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                    let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                    lock[job_id].updated_time = formatted_time;
                                    continue;
                                }
                                drop(lock);
                            }
                        }
                        let mut lock = JOB_LIST.lock().unwrap();
                        if check_right == 1 {
                            lock[job_id].cases[id_cnt as usize].result = String::from("Accepted");
                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                        } else {
                            lock[job_id].cases[id_cnt as usize].result = String::from("Wrong Answer");
                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                        }
                        //更新时间
                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                        lock[job_id].updated_time = formatted_time;
                        save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                        drop(lock);
                    }
                }
            }
            Some(packs) => {
                for pack in &packs {
                    let mut get_score = 1;
                    for i in pack {
                        id_cnt += 1;
                        //skip
                        if get_score == 0 {
                            let mut lock = JOB_LIST.lock().unwrap();
                            lock[job_id].cases[id_cnt as usize].result = String::from("Skipped");
                            lock[job_id].cases[id_cnt as usize].time = 0;
                            drop(lock);
                            continue;
                        }
                        let mut check_right = 1;
                        let in_file: File;
                        match File::open(temp_problem.cases[*i].input_file.clone()) {
                            Ok(temp_file) => {
                                in_file = temp_file;
                            }
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        let out_file: File;
                        let mut out_file_path = dir_path.clone();
                        out_file_path.push("output.txt");
                        match File::create(out_file_path) {
                            Ok(temp_file) => {
                                out_file = temp_file;
                            }
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        //运行该测试点
                        let case_start = Instant::now();
                        let time_limit_deration = Duration::from_micros(((temp_problem.cases[*i].time_limit as f64) * 1.05 ) as u64);
                        let status: std::process::ExitStatus;
                        let mut exe_path = dir_path.clone();
                        exe_path.push("test.exe");
                        let mut child = Command::new(&exe_path)
                        .stdin(Stdio::from(in_file))
                        .stdout(Stdio::from(out_file))
                        .stderr(Stdio::null())
                        .spawn().unwrap();
                        //判断超时
                        match child.wait_timeout(time_limit_deration).unwrap() {
                            Some(temp_status) => status = temp_status,
                            None => {
                                let case_duration = case_start.elapsed();
                                let processing_time = case_duration.as_micros();
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                drop(lock);
                                child.kill().unwrap();
                                let _ = child.wait().unwrap().code();
                                get_score = 0;
                                continue;
                            }
                        };
                        //记录运行用时
                        let case_duration = case_start.elapsed();
                        let processing_time = case_duration.as_micros();
                        //判断退出状态码
                        if let Some(code) = status.code() {
                            if code != 0 {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("Runtime Error");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                //更新时间
                                let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                lock[job_id].updated_time = formatted_time;
                                drop(lock);
                                get_score = 0;
                                continue;
                            }
                        }
                        //判断超时
                        if processing_time > temp_problem.cases[*i].time_limit {
                            get_score = 0;
                            let mut lock = JOB_LIST.lock().unwrap();
                            lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                            //更新时间
                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            lock[job_id].updated_time = formatted_time;
                            drop(lock);
                            continue;
                        }
                        else {
                            //未超时，对比输入输出
                            let mut out_file: File;
                            let mut out_file_path = dir_path.clone();
                            out_file_path.push("output.txt");
                            match File::open(out_file_path.clone()) {
                                Ok(temp_file) => {
                                    out_file = temp_file;
                                }
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            let mut ans_file: File;
                            match File::open(String::from(temp_problem.cases[*i].answer_file.clone())) {
                                Ok(temp_file) => {
                                    ans_file = temp_file;
                                }
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            let mut out_str = String::new();
                            let mut ans_str = String::new();
                            match out_file.read_to_string(&mut out_str) {
                                Ok(_num) => {}
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            match ans_file.read_to_string(&mut ans_str){
                                Ok(_num) => {}
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            if temp_problem.ty == "standard" {
                                let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                                if out_str_split.last().unwrap() == "" {
                                    out_str_split.pop();
                                }
                                let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                                if ans_str_split.last().unwrap() == "" {
                                    ans_str_split.pop();
                                }
                                if out_str_split.len() != ans_str_split.len() {
                                    check_right = 0;
                                } else {
                                    for j in 0..out_str_split.len() {
                                        if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                            check_right = 0;
                                            break;
                                        }
                                    }
                                }
                            }
                            else if temp_problem.ty == "strict" {
                                if out_str != ans_str {
                                    check_right = 0;
                                } 
                            }
                            else if temp_problem.ty == "dynamic_ranking" {
                                let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                                if out_str_split.last().unwrap() == "" {
                                    out_str_split.pop();
                                }
                                let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                                if ans_str_split.last().unwrap() == "" {
                                    ans_str_split.pop();
                                }
                                if out_str_split.len() != ans_str_split.len() {
                                    check_right = 0;
                                } else {
                                    for j in 0..out_str_split.len() {
                                        if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                            check_right = 0;
                                            break;
                                        }
                                    }
                                }
                            }
                            //temp_problem.ty == "spj"
                            else {  
                                let mut spj_out_path = dir_path.clone();
                                let mut spj_out_info: String = String::new();
                                spj_out_path.push("spj_out.txt");
                                let spj_out_file: File;
                                if let Ok(temp_file) = File::create(spj_out_path.clone()) {
                                    spj_out_file = temp_file;
                                } else {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                                let mut spj_command = temp_problem.misc.special_judge.clone().unwrap().clone();
                                for str in &mut spj_command {
                                    if str == "%OUTPUT%" {
                                        *str = out_file_path.clone().to_str().unwrap().to_string();
                                    }
                                    else if str == "%ANSWER%" {
                                        *str = temp_problem.cases[*i].answer_file.clone();
                                    }
                                }
                                let spj_command_slice = &spj_command[1..];
                                match Command::new(spj_command[0].clone()).args(spj_command_slice)
                                .stdout(Stdio::from(spj_out_file)).stderr(Stdio::null()).status() {
                                    Ok(temp_status) => {
                                        if temp_status.success() == false || temp_status.code() != Some(0) {
                                            let mut lock = JOB_LIST.lock().unwrap();
                                            lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                                            get_score = 0;
                                            //更新时间
                                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                            lock[job_id].updated_time = formatted_time;
                                            drop(lock);
                                            continue;
                                        }
                                    }
                                    Err(_err) => {
                                        let mut lock = JOB_LIST.lock().unwrap();
                                        lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                                        get_score = 0;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                        drop(lock);
                                        continue;
                                    }
                                }
                                let mut spj_out_file: File;
                                if let Ok(temp_file) = File::open(spj_out_path.clone()) {
                                    spj_out_file = temp_file;
                                } else {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                                match spj_out_file.read_to_string(&mut spj_out_info) {
                                    Ok(_num) => {}
                                    Err(_err) => {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                }
                                let mut spj_out_split: Vec<String> = spj_out_info.split('\n').map(|s| s.to_string()).collect();
                                if spj_out_split.last().unwrap() == "" {
                                    spj_out_split.pop();
                                }
                                if spj_out_split.len() != 2 {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                    //更新时间
                                    let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                    let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                    lock[job_id].updated_time = formatted_time;
                                    drop(lock);
                                    get_score = 0;
                                    continue;
                                }
                                else {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].info = spj_out_split[1].clone();
                                    if &spj_out_split[0] == "Accepted" {
                                        check_right = 1;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                    } else {
                                        lock[job_id].cases[id_cnt as usize].result = spj_out_split[0].clone();
                                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                                        get_score = 0;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                        continue;
                                    }
                                    drop(lock);
                                }
                            }
                            let mut lock = JOB_LIST.lock().unwrap();
                            if check_right == 1 {
                                lock[job_id].cases[id_cnt as usize].result = String::from("Accepted");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                            } else {
                                get_score = 0;
                                lock[job_id].cases[id_cnt as usize].result = String::from("Wrong Answer");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                            }
                            //更新时间
                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            lock[job_id].updated_time = formatted_time;
                            save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                            drop(lock);
                        }
                    }
                    if get_score == 1 {
                        let mut lock = JOB_LIST.lock().unwrap();
                        if temp_problem.misc.dynamic_ranking_ratio == None {
                            for i in pack {
                                lock[job_id].score += temp_problem.cases[*i].score;
                            }
                        }
                        else {
                            for i in pack {
                                lock[job_id].score += temp_problem.cases[*i].score * (1.0 - temp_problem.misc.dynamic_ranking_ratio.unwrap());
                            }
                        }
                        //更新时间
                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                        lock[job_id].updated_time = formatted_time;
                        save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                        drop(lock);
                    }
                }
            }
        }
        //更新 submission 的 result
        let mut lock = JOB_LIST.lock().unwrap();
        lock[job_id].state = String::from("Finished");
        if temp_problem.misc.dynamic_ranking_ratio == None {
            if lock[job_id].score as i32 == 100 {
                lock[job_id].result = String::from("Accepted");
            }
            else {
                for i in 1..lock[job_id].cases.len() {
                    if lock[job_id].cases[i].result != "Waiting" && 
                    lock[job_id].cases[i].result != "Accepted" {
                        lock[job_id].result = lock[job_id].cases[i].result.clone();
                        break;
                    }
                }
            }
        } 
        else {
            if lock[job_id].score.partial_cmp(&(100.0 * temp_problem.misc.dynamic_ranking_ratio.unwrap())).unwrap() == std::cmp::Ordering::Equal {
                lock[job_id].result = String::from("Accepted");
            }
            else {
                for i in 1..lock[job_id].cases.len() {
                    if lock[job_id].cases[i].result != "Waiting" && 
                    lock[job_id].cases[i].result != "Accepted" {
                        lock[job_id].result = lock[job_id].cases[i].result.clone();
                        break;
                    }
                }
            }
        }
        //更新时间
        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        lock[job_id].updated_time = formatted_time;
        save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
        drop(lock);
        //删除目录
        match std::fs::remove_dir_all(&dir_path) {
            Ok(()) => {}
            Err(_err) => {
                return Err("Internal Error".to_string());
            }
        }
        return Ok(());
                
    }
    }).await;
    if let Err(_err) = block_result {
        let mut lock = JOB_LIST.lock().unwrap();
        lock[job_id].result = String::from("System Error");
        drop(lock);
        return HttpResponse::InternalServerError().json(Error {
            code : 6,
            reason : String::from("ERR_INTERNAL"), 
            message: String::from("HTTP 500 Internal Server Error"),
        });
    } else {
        if let Ok(Err(_str)) = block_result {
            let mut lock = JOB_LIST.lock().unwrap();
            lock[job_id].result = String::from("System Error");
            drop(lock);
            return HttpResponse::InternalServerError().json(Error {
                code : 6,
                reason : String::from("ERR_INTERNAL"), 
                message: String::from("HTTP 500 Internal Server Error"),
            });
        }
        let lock = JOB_LIST.lock().unwrap();
        return HttpResponse::Ok().json(lock[job_id].clone());   
    }
    });
    let lock = JOB_LIST.lock().unwrap();
    return HttpResponse::Ok().json(lock[job_id].clone());
}
#[post("/internal/exit")]
#[allow(unreachable_code)]
async fn exit() -> impl Responder {
    log::info!("Shutdown as requested");
    std::process::exit(0);
    format!("Exited")
}
#[get("/jobs")]
async fn get_jobs(req: HttpRequest, secret_key: web::Data<DecodingKey>, if_token: web::Data<bool>) -> impl Responder {
    //鉴权
    if *if_token == true.into() {
        let deco_result = decoding(req.clone(), secret_key);
        if deco_result == None {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else if deco_result != Some(String::from("Administrator")) {
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Only Administrator have the right."),
            });
        }
    }
    let status_str = vec!["Queueing", "Running", "Finished", "Canceled"];
    let result_str = vec!["Waiting", "Running", "Accepted", "Compilation Error",
    "Compilation Success", "Wrong Answer", "Runtime Error","Time Limit Exceeded", 
    "Memory Limit Exceeded", "System Error", "SPJ Error", "Skipped"];
    let lock = JOB_LIST.lock().unwrap();
    let mut job_list_filted: Vec<JsonResponse> = lock.clone();
    drop(lock);
    let user_list = USERS.lock().unwrap();
    //解析 query
    //Adapted from https://stackoverflow.com/questions/54406029/how-can-i-parse-query-strings-in-actix-web by Rokit on 2023-07-14
    //get query in a HttpRequest
    let query_part = req.query_string();
    let mut url_params: HashMap<&str, &str> = HashMap::new();
    for pair in query_part.split('&') {
        let pair_inner: Vec<&str> = pair.split('=').collect();
        if pair_inner.len() == 2 {
            url_params.insert(pair_inner[0], pair_inner[1]);
        }
    }
    //filter
    for (key, value) in url_params {
        if key == "problem_id" {
            if let Ok(num) = value.parse::<i32>() {
                job_list_filted = job_list_filted.iter().
                filter(|&i| (*i).submission.problem_id == num).cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument problem_id"),
                });
            }
        }
        else if key == "user_id" {
            if let Ok(num) = value.parse::<i32>() {
                job_list_filted = job_list_filted.iter().
                filter(|&i| (*i).submission.user_id == num).cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument user_id"),
                });
            }
        }
        else if key == "contest_id" {
            if let Ok(num) = value.parse::<i32>() {
                job_list_filted = job_list_filted.iter().
                filter(|&i| (*i).submission.contest_id == num).cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument contest_id"),
                });
            }
        }
        else if key == "user_name" {
            if let Ok(name) = value.parse::<String>() {
                job_list_filted = job_list_filted.iter().
                filter(|&i| user_list[(*i).submission.user_id as usize].name == name).cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument user_name"),
                });
            }
        }
        else if key == "language" {
            if let Ok(language) = value.parse::<String>() {
                job_list_filted = job_list_filted.iter().
                filter(|&i| (*i).submission.language == language).cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument language"),
                });
            }
        }
        else if key == "from" {
            if let Ok(from_time) = value.parse::<String>() {
                //用正则表达式判断是否合法
                //Inspired from GPT
                let pattern = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z";
                let re = Regex::new(pattern).unwrap();
                if re.is_match(&from_time) == false {
                    return HttpResponse::BadRequest().json(Error {
                        code : 1,
                        reason : String::from("ERR_INVALID_ARGUMENT"), 
                        message : String::from("Invalid argument from"),
                    });
                }
                //filter
                let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
                let parsed_from_time = NaiveDateTime::parse_from_str(&from_time, format).unwrap();
                let utc_from_time: NaiveDateTime = parsed_from_time;
                job_list_filted = job_list_filted.iter().
                filter(|&i| NaiveDateTime::parse_from_str(&(*i).created_time, format).
                unwrap() >= utc_from_time)
                .cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument from"),
                });
            }
        }
        else if key == "to" {
            if let Ok(to_time) = value.parse::<String>() {
                //req_to = Some(to_time.clone());
                //用正则表达式判断是否合法
                let pattern = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z";
                let re = Regex::new(pattern).unwrap();
                if re.is_match(&to_time) == false {
                    return HttpResponse::BadRequest().json(Error {
                        code : 1,
                        reason : String::from("ERR_INVALID_ARGUMENT"), 
                        message : String::from("Invalid argument to"),
                    });
                }
                //filter
                let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
                let parsed_to_time = NaiveDateTime::parse_from_str(&to_time, format).unwrap();
                let utc_to_time = parsed_to_time;
                job_list_filted = job_list_filted.iter().
                filter(|&i| NaiveDateTime::parse_from_str(&(*i).created_time, format).
                unwrap() <= utc_to_time)
                .cloned().collect();
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument to"),
                });
            }
        }
        else if key == "state" {
            if let Ok(state) = value.parse::<String>() {
                match status_str.iter().find(|s| s == &&&state) {
                    Some(_) => {
                        job_list_filted = job_list_filted.iter().
                        filter(|&i| (*i).state == state).cloned().collect();
                    }
                    None => {
                        return HttpResponse::BadRequest().json(Error {
                            code : 1,
                            reason : String::from("ERR_INVALID_ARGUMENT"), 
                            message : String::from("Invalid argument state"),
                        });
                    }
                }
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument state"),
                });
            }
        }
        else if key == "result" {
            if let Ok(result) = value.parse::<String>() {
                match result_str.iter().find(|s| s == &&&result) {
                    Some(_) => {
                        job_list_filted = job_list_filted.iter().
                        filter(|&i| (*i).result == result).cloned().collect();
                    }
                    None => {
                        return HttpResponse::BadRequest().json(Error {
                            code : 1,
                            reason : String::from("ERR_INVALID_ARGUMENT"), 
                            message : String::from("Invalid argument result"),
                        });
                    }
                }
            }
            else {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid argument result"),
                });
            }
        }
        else {
            let mut message_str = String::from("Invalid argument ");
            message_str.push_str(&key);
            return HttpResponse::BadRequest().json(Error {
                code : 1,
                reason : String::from("ERR_INVALID_ARGUMENT"), 
                message : message_str,
            });
        }
    }
    //sort
    let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
    if job_list_filted.len() > 0 {
        job_list_filted.sort_by(move |a, b| 
        NaiveDateTime::parse_from_str(&((*a).created_time), format)
        .unwrap()
        .cmp(&(NaiveDateTime::parse_from_str(&((*b).created_time), format)
        .unwrap())));
    }

    return HttpResponse::Ok().json(job_list_filted);
}
#[get("/jobs/{jobId}")]
async fn get_job_id(job_id: web::Path<String>, req: HttpRequest, secret_key: web::Data<DecodingKey>, if_token: web::Data<bool>) -> impl Responder {
    let job_id_str: String = job_id.to_string();
    //鉴权
    if *if_token == true.into() {
        let deco_result = decoding(req, secret_key);
        if deco_result == None {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else if deco_result != Some(String::from("Administrator")) {
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Only Administrator have the right."),
            });
        }
    }
    let job_id_usize: usize = job_id_str.parse().unwrap();
    let job_list = JOB_LIST.lock().unwrap();
    if job_id_usize < job_list.len() {
        return HttpResponse::Ok().json(job_list[job_id_usize].clone());
    } else {
        let mut message_str = String::from("Job ");
        message_str.push_str(&job_id_str);
        message_str.push_str(" not found.");
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : message_str,
        });
    }
}
#[put("/jobs/{jobId}")]
async fn put_job_id(job_id_web: web::Path<String>, setting: web::Data<Setting>, req: HttpRequest, 
secret_key: web::Data<DecodingKey>, if_token: web::Data<bool>) -> impl Responder {
    //鉴权
    if *if_token == true.into() {
        let deco_result = decoding(req, secret_key);
        if deco_result == None {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else if deco_result != Some(String::from("Author")) {
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Only Author have the right."),
            });
        }
    }
    let job_id_str: String = job_id_web.to_string();
    let job_id: usize = job_id_str.parse().unwrap();
    let mut lock = JOB_LIST.lock().unwrap();
    //任务不存在
    if job_id >= lock.len() {
        let mut message_str = String::from("Job ");
        message_str.push_str(&job_id_str);
        message_str.push_str(" not found.");
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : message_str,
        });
    }
    //任务 not finished
    if lock[job_id].state != String::from("Finished") {
        let mut message_str = String::from("Job ");
        message_str.push_str(&job_id_str);
        message_str.push_str(" not finished.");
        return HttpResponse::BadRequest().json(Error {
            code : 2,
            reason : String::from("ERR_INVALID_STATE"), 
            message : message_str,
        });
    }
    else { 
        //开始重新测评
        let mut temp_problem: Problem = Problem { id:0, name: String::new(), ty: String::new(), misc: Misc { packing: None, special_judge: None, dynamic_ranking_ratio: None }, cases: Vec::new() };
        let mut temp_language: Language = Language { name:String::new(), file_name: String::new(), command: vec![] };
        let mut check_lan = 0;
        let mut check_prob_id = 0;
        let mut check_user_id = 0;
        let mut check_contest_id = 1;
        let mut dir_path = PathBuf::new();
        dir_path.push(String::from("target"));
        dir_path.push(format!("tmp_{}", &job_id.to_string()));
        let mut temp_code_file = dir_path.clone();
        //检查
        for i in &setting.languages {
            if i.name == lock[job_id].submission.language {
                check_lan = 1;
                temp_code_file.push(&i.file_name);
                temp_language = i.clone();
                break;
            }
        }
        for i in &setting.problems {
            if i.id == lock[job_id].submission.problem_id {
                check_prob_id = 1;
                temp_problem = i.clone();
                break;
            }
        }
        let user_list = USERS.lock().unwrap();
        for i in user_list.iter() {
            if i.id == Some(lock[job_id].submission.user_id) {
                check_user_id = 1;
            }
        }
        drop(user_list);
        let contest_list = CONTESTS.lock().unwrap();
        if lock[job_id].submission.contest_id > contest_list.len() as i32 {
            check_contest_id = 0;
        }
        if check_lan == 0 || check_prob_id == 0 || check_user_id == 0 || check_contest_id == 0 {
            let _ = std::fs::remove_dir_all(&dir_path);
            return HttpResponse::NotFound().json(Error {
                code : 3,
                reason : String::from("ERR_NOT_FOUND"), 
                message : String::from("HTTP 404 Not Found"),
            });
        }
        drop(contest_list);
        lock[job_id].state = String::from("Queueing");
        lock[job_id].result = String::from("Waiting");
        lock[job_id].score = 0.0;
        lock[job_id].cases = Vec::new();
        for i in 0..temp_problem.cases.len() + 1 {
            let temp_case: CaseReturn = CaseReturn { id: i as i32, result: String::from("Waiting"), 
            time: 0, memory: 0, info: String::from("") };
            lock[job_id].cases.push(temp_case);
        }
        save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
        drop(lock);
        //进入异步
        actix_web::rt::spawn(async move {
        let block_result = actix_web::web::block(move ||{
        
        
        //删、建文件夹（忽略“找不到文件夹”的错误）
        let _ = std::fs::remove_dir_all(&dir_path);
        match std::fs::create_dir(&dir_path) {
            Ok(()) => {}
            Err(_err) => { return Err("Internal Error".to_string()); }
        }
        //将源代码写入 temp_code_file
        let mut f: File;
        match std::fs::File::create(temp_code_file.clone()) {
            Ok(temp_f) => {f = temp_f}
            Err(_err) => {
                let _ = std::fs::remove_dir_all(&dir_path);
                return Err("Internal Error".to_string()); 
            }
        }
        let lock = JOB_LIST.lock().unwrap();
        match f.write(lock[job_id].submission.source_code.as_bytes()) {
            Ok(_num) => {}
            Err(_err) => {
                let _ = std::fs::remove_dir_all(&dir_path);
                return Err("Internal Error".to_string());
            }
        }
        drop(lock);
        //构建编译 command
        let mut command_clone = temp_language.command;
        for j in &mut command_clone {
            if j == "%OUTPUT%" {
                *j = dir_path.clone().to_str().unwrap().to_string();
                (*j).push_str("/test.exe");
                break;
            }
        }
        for j in &mut command_clone {
            if j == "%INPUT%" {
                *j = temp_code_file.clone().to_str().unwrap().to_string();
                break;
            }
        }
        let mut lock = JOB_LIST.lock().unwrap();
        //编译
        lock[job_id].state = String::from("Running");
        lock[job_id].result = String::from("Running");
        lock[job_id].cases[0].result = String::from("Running");
        drop(lock);
        let command_clone_slice = &command_clone[1..];
        let compile_start = Instant::now();
        let status: std::process::ExitStatus;
        match Command::new(command_clone[0].clone()).args(command_clone_slice)
        .status() {
            Ok(temp_status) => {
                status = temp_status;
            }
            Err(_err) => {//无法运行编译器
                let _ = std::fs::remove_dir_all(&dir_path);
                return Err("Internal Error".to_string());
            }
        }
        let compile_duration = compile_start.elapsed();
        
        //编译失败（编译器异常退出）
        if status.success() == false {
            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            let mut lock = JOB_LIST.lock().unwrap();
            lock[job_id].updated_time = formatted_time;
            lock[job_id].state = String::from("Finished");
            lock[job_id].result = String::from("Compilation Error");
            lock[job_id].cases[0].result = String::from("Compilation Error");
            lock[job_id].cases[0].time = compile_duration.as_micros();
            let _ = std::fs::remove_dir_all(&dir_path);
            save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
            drop(lock);
            return Ok(());
        } //编译成功，进入循环判断 cases
        else {
            let mut lock = JOB_LIST.lock().unwrap();
            lock[job_id].cases[0].result = String::from("Compilation Success");
            lock[job_id].cases[0].time = compile_duration.as_micros();
            drop(lock);
            let mut id_cnt = 0;
            match temp_problem.misc.packing {
                None => {
                    for i in &mut temp_problem.cases {
                        id_cnt += 1;
                        let mut check_right = 1;
                        let in_file: File;
                        match File::open(i.input_file.clone()) {
                            Ok(temp_file) => {
                                in_file = temp_file;
                            }
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        let out_file: File;
                        let mut out_file_path = dir_path.clone();
                        out_file_path.push("output.txt");
                        match File::create(out_file_path) {
                            Ok(temp_file) => {
                                out_file = temp_file;
                            }
                            Err(_err) => {
                                let _ = std::fs::remove_dir_all(&dir_path);
                                return Err("Internal Error".to_string());
                            }
                        }
                        //运行该测试点
                        let case_start = Instant::now();
                        let time_limit_deration = Duration::from_micros(((i.time_limit as f64) * 1.05 ) as u64);
                        let status: std::process::ExitStatus;
                        let mut exe_path = dir_path.clone();
                        exe_path.push("test.exe");
                        let mut child = Command::new(&exe_path)
                        .stdin(Stdio::from(in_file))
                        .stdout(Stdio::from(out_file))
                        .stderr(Stdio::null())
                        .spawn().unwrap();
                        //判断超时
                        match child.wait_timeout(time_limit_deration).unwrap() {
                            Some(temp_status) => status = temp_status,
                            None => {
                                let case_duration = case_start.elapsed();
                                let processing_time = case_duration.as_micros();
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                drop(lock);
                                child.kill().unwrap();
                                let _ = child.wait().unwrap().code();
                                continue;
                            }
                        };
                        //记录运行用时
                        let case_duration = case_start.elapsed();
                        let processing_time = case_duration.as_micros();
                        //判断退出状态码
                        if let Some(code) = status.code() {
                            if code != 0 {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("Runtime Error");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                //更新时间
                                let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                lock[job_id].updated_time = formatted_time;
                                save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                                drop(lock);
                                continue;
                            }
                        }
                        //判断超时
                        if processing_time > i.time_limit {
                            let mut lock = JOB_LIST.lock().unwrap();
                            lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                            //更新时间
                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            lock[job_id].updated_time = formatted_time;
                            drop(lock);
                            continue;
                        }
                        else {
                            //未超时，对比输入输出
                            let mut out_file: File;
                            let mut out_file_path = dir_path.clone();
                            out_file_path.push("output.txt");
                            match File::open(out_file_path.clone()) {
                                Ok(temp_file) => {
                                    out_file = temp_file;
                                }
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            let mut ans_file: File;
                            match File::open(String::from(i.answer_file.clone())) {
                                Ok(temp_file) => {
                                    ans_file = temp_file;
                                }
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            let mut out_str = String::new();
                            let mut ans_str = String::new();
                            match out_file.read_to_string(&mut out_str) {
                                Ok(_num) => {}
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            match ans_file.read_to_string(&mut ans_str){
                                Ok(_num) => {}
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            if temp_problem.ty == "standard" {
                                let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                                if out_str_split.last().unwrap() == "" {
                                    out_str_split.pop();
                                }
                                let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                                if ans_str_split.last().unwrap() == "" {
                                    ans_str_split.pop();
                                }
                                if out_str_split.len() != ans_str_split.len() {
                                    check_right = 0;
                                } else {
                                    for j in 0..out_str_split.len() {
                                        if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                            check_right = 0;
                                            break;
                                        }
                                    }
                                }
                                if check_right == 1 {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].score += i.score;
                                    drop(lock);
                                } 
                            }
                            else if temp_problem.ty == "strict" {
                                if out_str != ans_str {
                                    check_right = 0;
                                }
                                else {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].score += i.score;
                                    drop(lock);
                                }
                            }
                            else if temp_problem.ty == "dynamic_ranking" {
                                let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                                if out_str_split.last().unwrap() == "" {
                                    out_str_split.pop();
                                }
                                let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                                if ans_str_split.last().unwrap() == "" {
                                    ans_str_split.pop();
                                }
                                if out_str_split.len() != ans_str_split.len() {
                                    check_right = 0;
                                } else {
                                    for j in 0..out_str_split.len() {
                                        if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                            check_right = 0;
                                            break;
                                        }
                                    }
                                }
                                if check_right == 1 {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].score += i.score * (1.0 - temp_problem.misc.dynamic_ranking_ratio.unwrap());
                                    drop(lock);
                                }
                            }
                            //temp_problem.ty == "spj"
                            else {  
                                let mut spj_out_path = dir_path.clone();
                                let mut spj_out_info: String = String::new();
                                spj_out_path.push("spj_out.txt");
                                let spj_out_file: File;
                                if let Ok(temp_file) = File::create(spj_out_path.clone()) {
                                    spj_out_file = temp_file;
                                } else {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                                let mut spj_command = temp_problem.misc.special_judge.clone().unwrap().clone();
                                for str in &mut spj_command {
                                    if str == "%OUTPUT%" {
                                        *str = out_file_path.clone().to_str().unwrap().to_string();
                                    }
                                    else if str == "%ANSWER%" {
                                        *str = i.answer_file.clone();
                                    }
                                }
                                let spj_command_slice = &spj_command[1..];
                                match Command::new(spj_command[0].clone()).args(spj_command_slice)
                                .stdout(Stdio::from(spj_out_file)).stderr(Stdio::null()).status() {
                                    Ok(temp_status) => {
                                        if temp_status.success() == false || temp_status.code() != Some(0) {
                                            let mut lock = JOB_LIST.lock().unwrap();
                                            lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                                            //更新时间
                                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                            lock[job_id].updated_time = formatted_time;
                                            drop(lock);
                                            continue;
                                        }
                                    }
                                    Err(_err) => {
                                        let mut lock = JOB_LIST.lock().unwrap();
                                        lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                        drop(lock);
                                        continue;
                                    }
                                }
                                let mut spj_out_file: File;
                                if let Ok(temp_file) = File::open(spj_out_path.clone()) {
                                    spj_out_file = temp_file;
                                } else {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string())
                                }
                                match spj_out_file.read_to_string(&mut spj_out_info) {
                                    Ok(_num) => {}
                                    Err(_err) => {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                }
                                let mut spj_out_split: Vec<String> = spj_out_info.split('\n').map(|s| s.to_string()).collect();
                                if spj_out_split.last().unwrap() == "" {
                                    spj_out_split.pop();
                                }
                                if spj_out_split.len() != 2 {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                    //更新时间
                                    let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                    let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                    lock[job_id].updated_time = formatted_time;
                                    drop(lock);
                                    continue;
                                }
                                else {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].info = spj_out_split[1].clone();
                                    if &spj_out_split[0] == "Accepted" {
                                        check_right = 1;
                                        lock[job_id].score += i.score;
                                        
                                    } else {
                                        lock[job_id].cases[id_cnt as usize].result = spj_out_split[0].clone();
                                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                        continue;
                                    }
                                    drop(lock);
                                }
                            }
                            let mut lock = JOB_LIST.lock().unwrap();
                            if check_right == 1 {
                                lock[job_id].cases[id_cnt as usize].result = String::from("Accepted");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                            } else {
                                lock[job_id].cases[id_cnt as usize].result = String::from("Wrong Answer");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                            }
                            //更新时间
                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            lock[job_id].updated_time = formatted_time;
                            save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                            drop(lock);
                        }
                    }
                }
                Some(packs) => {
                    for pack in &packs {
                        let mut get_score = 1;
                        for i in pack {
                            id_cnt += 1;
                            //skip
                            if get_score == 0 {
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("Skipped");
                                lock[job_id].cases[id_cnt as usize].time = 0;
                                drop(lock);
                                continue;
                            }
                            let mut check_right = 1;
                            let in_file: File;
                            match File::open(temp_problem.cases[*i].input_file.clone()) {
                                Ok(temp_file) => {
                                    in_file = temp_file;
                                }
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            let out_file: File;
                            let mut out_file_path = dir_path.clone();
                            out_file_path.push("output.txt");
                            match File::create(out_file_path) {
                                Ok(temp_file) => {
                                    out_file = temp_file;
                                }
                                Err(_err) => {
                                    let _ = std::fs::remove_dir_all(&dir_path);
                                    return Err("Internal Error".to_string());
                                }
                            }
                            //运行该测试点
                            let case_start = Instant::now();
                            let time_limit_deration = Duration::from_micros(((temp_problem.cases[*i].time_limit as f64) * 1.05 ) as u64);
                            let status: std::process::ExitStatus;
                            let mut exe_path = dir_path.clone();
                            exe_path.push("test.exe");
                            let mut child = Command::new(&exe_path)
                            .stdin(Stdio::from(in_file))
                            .stdout(Stdio::from(out_file))
                            .stderr(Stdio::null())
                            .spawn().unwrap();
                            //判断超时
                            match child.wait_timeout(time_limit_deration).unwrap() {
                                Some(temp_status) => status = temp_status,
                                None => {
                                    let case_duration = case_start.elapsed();
                                    let processing_time = case_duration.as_micros();
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                    drop(lock);
                                    child.kill().unwrap();
                                    let _ = child.wait().unwrap().code();
                                    get_score = 0;
                                    continue;
                                }
                            };
                            //记录运行用时
                            let case_duration = case_start.elapsed();
                            let processing_time = case_duration.as_micros();
                            //判断退出状态码
                            if let Some(code) = status.code() {
                                if code != 0 {
                                    let mut lock = JOB_LIST.lock().unwrap();
                                    lock[job_id].cases[id_cnt as usize].result = String::from("Runtime Error");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                    //更新时间
                                    let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                    let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                    lock[job_id].updated_time = formatted_time;
                                    drop(lock);
                                    get_score = 0;
                                    continue;
                                }
                            }
                            //判断超时
                            if processing_time > temp_problem.cases[*i].time_limit {
                                get_score = 0;
                                let mut lock = JOB_LIST.lock().unwrap();
                                lock[job_id].cases[id_cnt as usize].result = String::from("Time Limit Exceeded");
                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                //更新时间
                                let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                lock[job_id].updated_time = formatted_time;
                                drop(lock);
                                continue;
                            }
                            else {
                                //未超时，对比输入输出
                                let mut out_file: File;
                                let mut out_file_path = dir_path.clone();
                                out_file_path.push("output.txt");
                                match File::open(out_file_path.clone()) {
                                    Ok(temp_file) => {
                                        out_file = temp_file;
                                    }
                                    Err(_err) => {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                }
                                let mut ans_file: File;
                                match File::open(String::from(temp_problem.cases[*i].answer_file.clone())) {
                                    Ok(temp_file) => {
                                        ans_file = temp_file;
                                    }
                                    Err(_err) => {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                }
                                let mut out_str = String::new();
                                let mut ans_str = String::new();
                                match out_file.read_to_string(&mut out_str) {
                                    Ok(_num) => {}
                                    Err(_err) => {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                }
                                match ans_file.read_to_string(&mut ans_str){
                                    Ok(_num) => {}
                                    Err(_err) => {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                }
                                if temp_problem.ty == "standard" {
                                    let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                                    if out_str_split.last().unwrap() == "" {
                                        out_str_split.pop();
                                    }
                                    let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                                    if ans_str_split.last().unwrap() == "" {
                                        ans_str_split.pop();
                                    }
                                    if out_str_split.len() != ans_str_split.len() {
                                        check_right = 0;
                                    } else {
                                        for j in 0..out_str_split.len() {
                                            if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                                check_right = 0;
                                                break;
                                            }
                                        }
                                    }
                                }
                                else if temp_problem.ty == "strict" {
                                    if out_str != ans_str {
                                        check_right = 0;
                                    } 
                                }
                                else if temp_problem.ty == "dynamic_ranking" {
                                    let mut out_str_split: Vec<String> = out_str.split('\n').map(|s| s.to_string()).collect();
                                    if out_str_split.last().unwrap() == "" {
                                        out_str_split.pop();
                                    }
                                    let mut ans_str_split: Vec<String>  = ans_str.split('\n').map(|s| s.to_string()).collect();
                                    if ans_str_split.last().unwrap() == "" {
                                        ans_str_split.pop();
                                    }
                                    if out_str_split.len() != ans_str_split.len() {
                                        check_right = 0;
                                    } else {
                                        for j in 0..out_str_split.len() {
                                            if out_str_split[j].trim_end() != ans_str_split[j].trim_end() {
                                                check_right = 0;
                                                break;
                                            }
                                        }
                                    }
                                }
                                //temp_problem.ty == "spj"
                                else {  
                                    let mut spj_out_path = dir_path.clone();
                                    let mut spj_out_info: String = String::new();
                                    spj_out_path.push("spj_out.txt");
                                    let spj_out_file: File;
                                    if let Ok(temp_file) = File::create(spj_out_path.clone()) {
                                        spj_out_file = temp_file;
                                    } else {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                    let mut spj_command = temp_problem.misc.special_judge.clone().unwrap().clone();
                                    for str in &mut spj_command {
                                        if str == "%OUTPUT%" {
                                            *str = out_file_path.clone().to_str().unwrap().to_string();
                                        }
                                        else if str == "%ANSWER%" {
                                            *str = temp_problem.cases[*i].answer_file.clone();
                                        }
                                    }
                                    let spj_command_slice = &spj_command[1..];
                                    match Command::new(spj_command[0].clone()).args(spj_command_slice)
                                    .stdout(Stdio::from(spj_out_file)).stderr(Stdio::null()).status() {
                                        Ok(temp_status) => {
                                            if temp_status.success() == false || temp_status.code() != Some(0) {
                                                let mut lock = JOB_LIST.lock().unwrap();
                                                lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                                lock[job_id].cases[id_cnt as usize].time = processing_time;
                                                get_score = 0;
                                                //更新时间
                                                let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                                let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                                lock[job_id].updated_time = formatted_time;
                                                drop(lock);
                                                continue;
                                            }
                                        }
                                        Err(_err) => {
                                            let mut lock = JOB_LIST.lock().unwrap();
                                            lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                                            get_score = 0;
                                            //更新时间
                                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                            lock[job_id].updated_time = formatted_time;
                                            drop(lock);
                                            continue;
                                        }
                                    }
                                    let mut spj_out_file: File;
                                    if let Ok(temp_file) = File::open(spj_out_path.clone()) {
                                        spj_out_file = temp_file;
                                    } else {
                                        let _ = std::fs::remove_dir_all(&dir_path);
                                        return Err("Internal Error".to_string());
                                    }
                                    match spj_out_file.read_to_string(&mut spj_out_info) {
                                        Ok(_num) => {}
                                        Err(_err) => {
                                            let _ = std::fs::remove_dir_all(&dir_path);
                                            return Err("Internal Error".to_string());
                                        }
                                    }
                                    let mut spj_out_split: Vec<String> = spj_out_info.split('\n').map(|s| s.to_string()).collect();
                                    if spj_out_split.last().unwrap() == "" {
                                        spj_out_split.pop();
                                    }
                                    if spj_out_split.len() != 2 {
                                        let mut lock = JOB_LIST.lock().unwrap();
                                        lock[job_id].cases[id_cnt as usize].result = String::from("SPJ Error");
                                        lock[job_id].cases[id_cnt as usize].time = processing_time;
                                        //更新时间
                                        let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                        let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                        lock[job_id].updated_time = formatted_time;
                                        drop(lock);
                                        get_score = 0;
                                        continue;
                                    }
                                    else {
                                        let mut lock = JOB_LIST.lock().unwrap();
                                        lock[job_id].cases[id_cnt as usize].info = spj_out_split[1].clone();
                                        if &spj_out_split[0] == "Accepted" {
                                            check_right = 1;
                                            //更新时间
                                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                            lock[job_id].updated_time = formatted_time;
                                        } else {
                                            lock[job_id].cases[id_cnt as usize].result = spj_out_split[0].clone();
                                            lock[job_id].cases[id_cnt as usize].time = processing_time;
                                            get_score = 0;
                                            //更新时间
                                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                            lock[job_id].updated_time = formatted_time;
                                            continue;
                                        }
                                        drop(lock);
                                    }
                                }
                                let mut lock = JOB_LIST.lock().unwrap();
                                if check_right == 1 {
                                    lock[job_id].cases[id_cnt as usize].result = String::from("Accepted");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                } else {
                                    get_score = 0;
                                    lock[job_id].cases[id_cnt as usize].result = String::from("Wrong Answer");
                                    lock[job_id].cases[id_cnt as usize].time = processing_time;
                                }
                                //更新时间
                                let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                                let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                                lock[job_id].updated_time = formatted_time;
                                save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                                drop(lock);
                            }
                        }
                        if get_score == 1 {
                            let mut lock = JOB_LIST.lock().unwrap();
                            if temp_problem.misc.dynamic_ranking_ratio == None {
                                for i in pack {
                                    lock[job_id].score += temp_problem.cases[*i].score;
                                }
                            }
                            else {
                                for i in pack {
                                    lock[job_id].score += temp_problem.cases[*i].score * (1.0 - temp_problem.misc.dynamic_ranking_ratio.unwrap());
                                }
                            }
                            //更新时间
                            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
                            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            lock[job_id].updated_time = formatted_time;
                            save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
                            drop(lock);
                        }
                    }
                }
            }
            //更新 submission 的 result
            let mut lock = JOB_LIST.lock().unwrap();
            lock[job_id].state = String::from("Finished");
            if temp_problem.misc.dynamic_ranking_ratio == None {
                if lock[job_id].score as i32 == 100 {
                    lock[job_id].result = String::from("Accepted");
                }
                else {
                    for i in 1..lock[job_id].cases.len() {
                        if lock[job_id].cases[i].result != "Waiting" && 
                        lock[job_id].cases[i].result != "Accepted" {
                            lock[job_id].result = lock[job_id].cases[i].result.clone();
                            break;
                        }
                    }
                }
            } 
            else {
                if lock[job_id].score.partial_cmp(&(100.0 * temp_problem.misc.dynamic_ranking_ratio.unwrap())).unwrap() == std::cmp::Ordering::Equal {
                    lock[job_id].result = String::from("Accepted");
                }
                else {
                    for i in 1..lock[job_id].cases.len() {
                        if lock[job_id].cases[i].result != "Waiting" && 
                        lock[job_id].cases[i].result != "Accepted" {
                            lock[job_id].result = lock[job_id].cases[i].result.clone();
                            break;
                        }
                    }
                }
            }
            //更新时间
            let utc_time_update: DateTime<Utc> = Utc::now(); // 获取当前 UTC 时间
            let formatted_time = utc_time_update.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
            lock[job_id].updated_time = formatted_time;
            save_job_list((*lock.clone()).to_vec(), "job_list_saved.json");
            drop(lock);
            //删除目录
            match std::fs::remove_dir_all(&dir_path) {
                Ok(()) => {}
                Err(_err) => {
                    return Err("Internal Error".to_string());
                }
            }
            return Ok(());
                    
        }
        }).await;
        if let Err(_err) = block_result {
            let mut lock = JOB_LIST.lock().unwrap();
            lock[job_id].result = String::from("System Error");
            drop(lock);
            return HttpResponse::InternalServerError().json(Error {
                code : 6,
                reason : String::from("ERR_INTERNAL"), 
                message: String::from("HTTP 500 Internal Server Error"),
            });
        } else {
            if let Ok(Err(_str)) = block_result {
                let mut lock = JOB_LIST.lock().unwrap();
                lock[job_id].result = String::from("System Error");
                drop(lock);
                return HttpResponse::InternalServerError().json(Error {
                    code : 6,
                    reason : String::from("ERR_INTERNAL"), 
                    message: String::from("HTTP 500 Internal Server Error"),
                });
            }
            let lock = JOB_LIST.lock().unwrap();
            return HttpResponse::Ok().json(lock[job_id].clone());   
        }
        });
        let lock = JOB_LIST.lock().unwrap();
        return HttpResponse::Ok().json(lock[job_id].clone());
    }
}
#[post("/users")]
async fn post_users(user: web::Json<User>, if_token: web::Data<bool>) -> impl Responder {
    if *if_token == true.into() {
        return HttpResponse::BadRequest().json(Error {
            code : 7,
            reason : String::from("ERR_INVALID_TOKENT"), 
            message: String::from("In user-management mode you cannot use POST/users."),
        });
    }
    let mut user_list = USERS.lock().unwrap();
    match user.id {
        None => {
            for user_saved in user_list.iter() {
                if user_saved.name == user.name {
                    let mut message_str = String::from("User name '");
                    message_str.push_str(&user.name);
                    message_str.push_str("' already exists.");
                    return HttpResponse::BadRequest().json(Error {
                        code : 1,
                        reason : String::from("ERR_INVALID_ARGUMENT"), 
                        message : message_str,
                    });
                }
            };
            let new_user: User = User {id: Some(user_list.len() as i32), name: user.name.clone()};
            user_list.push(new_user.clone());
            return HttpResponse::Ok().json(new_user.clone());
        }
        Some( user_id ) => {
            //对应 id 的 user 不存在
            if user_id as usize >= user_list.len() {
                let mut message_str = String::from("User ");
                message_str.push_str(&user_id.to_string());
                message_str.push_str(" not found.");
                return HttpResponse::NotFound().json(Error {
                    code : 3,
                    reason : String::from("ERR_NOT_FOUND"), 
                    message : message_str,
                });
            }
            //判断重名
            for user_saved in user_list.iter() {
                if user_saved.name == user.name && user_saved.id != Some(user_id) {
                    //User 后面有空格吗
                    let mut message_str = String::from("User name '");
                    message_str.push_str(&user.name);
                    message_str.push_str("' already exists.");
                    return HttpResponse::BadRequest().json(Error {
                        code : 1,
                        reason : String::from("ERR_INVALID_ARGUMENT"), 
                        message : message_str,
                    });
                }
            };
            user_list[user_id as usize].name = user.name.clone();
            save_user_list((*user_list.clone()).to_vec(), "user_list_saved.json");
            return HttpResponse::Ok().json(user_list[user_id as usize].clone());
        }
    }
}
#[get("/users")]
async fn get_users(req: HttpRequest, secret_key: web::Data<DecodingKey>, if_token: web::Data<bool>) -> impl Responder {
    //鉴权
    if *if_token == true.into() {
        let deco_result = decoding(req, secret_key);
        if deco_result == None {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else if deco_result != Some(String::from("Administrator")) {
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Only Administrator have the right."),
            });
        }
    }
    let user_list = USERS.lock().unwrap();
    return HttpResponse::Ok().json(user_list.clone());
}
#[get("/contests/{contestID}/ranklist")]
async fn get_contests_id_ranklist(req: HttpRequest, contest_id_arg: web::Path<String>, 
setting: web::Data<Setting>) -> impl Responder {
    let contest_id_str: String = contest_id_arg.to_string();
    let contest_id: i32 = contest_id_str.parse().unwrap();
    let mut users_in_contest: Vec<UserInContest> = Vec::new();
    let whole_user_list = USERS.lock().unwrap();
    let job_list = JOB_LIST.lock().unwrap();
    let contest_list = CONTESTS.lock().unwrap();
    let mut scoring_rule: String = String::from("latest");
    let mut tie_breaker: String = String::from("no");
    //解析 query
    let query_part = req.query_string();
    for pair in query_part.split('&') {
        let pair_inner: Vec<&str> = pair.split('=').collect();
        if pair_inner.len() == 2 {
            if pair_inner[0] == "scoring_rule" {
                scoring_rule = String::from(pair_inner[1]);
            } 
            else if  pair_inner[0] == "tie_breaker" {
                tie_breaker = String::from(pair_inner[1]);
            }
        }
    }
    //全局排行
    if contest_id == 0 {
        //构建 users_in_contest
        for user_saved in whole_user_list.iter() {
            let new_performance = Performance {
                if_did: false, score: 0.0, submission_time: String::from("-1"), submission_count: 0, job_id: 0
            };
            let mut performance_pair: HashMap<i32, Performance> = HashMap::new();
            for problem in &setting.problems {
                performance_pair.entry(problem.id).or_insert(new_performance.clone());
            } 
            let new_user_in_contest: UserInContest = UserInContest {
                user_info: user_saved.clone(), performances: performance_pair.clone(), 
                total_score: 0.0, submssion_time: String::from("-1"), total_submission_count: 0, rank: 0
            };
            users_in_contest.push(new_user_in_contest);
        }
    } 
    else if contest_id > contest_list.len() as i32 {
        let mut message_str = String::from("Contest ");
        message_str.push_str(&contest_id.to_string());
        message_str.push_str(" not found.");
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : message_str,
        });
    }
    else {
        //构建 users_in_contest
        for i in &contest_list[contest_id as usize - 1].user_ids {
            let new_performance = Performance {
                if_did: false, score: 0.0, submission_time: String::from("-1"), submission_count: 0, job_id: 0
            };
            let mut performance_pair: HashMap<i32, Performance> = HashMap::new();
            for problem in &contest_list[contest_id as usize - 1].problem_ids {
                performance_pair.entry(*problem).or_insert(new_performance.clone());
            } 
            let new_user_in_contest: UserInContest = UserInContest {
                user_info: whole_user_list[*i as usize].clone(), performances: performance_pair.clone(), 
                total_score: 0.0, submssion_time: String::from("-1"), total_submission_count: 0, rank: 0
            };
            users_in_contest.push(new_user_in_contest);
        }
    }
    //存储“是否竞争得分”的信息
    let mut if_dyn: HashMap<i32, bool> = HashMap::new();
    for problem in &setting.problems {
        if problem.misc.dynamic_ranking_ratio == None {
            if_dyn.entry(problem.id).or_insert(false);
        } else {
            if_dyn.entry(problem.id).or_insert(true);
        }
    }
    //求“用哪次提交来算分”
    if &scoring_rule == "latest" {
        for user in &mut users_in_contest {
            let mut job_cnt: i32 = 0;
            for job in job_list.iter() {
                if Some(job.submission.user_id) == user.user_info.id && 
                user.performances.contains_key(&job.submission.problem_id) {
                    let performance_temp = user.performances.get_mut(&job.submission.problem_id).unwrap();
                    if performance_temp.if_did == false {
                        performance_temp.if_did = true;
                        performance_temp.score = job.score;
                        performance_temp.submission_time = job.created_time.clone();
                        performance_temp.submission_count += 1;
                        performance_temp.job_id = job_cnt;
                    } else {
                        performance_temp.submission_count += 1;
                        let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
                        //有竞争得分
                        if *(if_dyn.get(&job.submission.problem_id).unwrap()) == true {
                            if &(job_list[performance_temp.job_id as usize].result) != "Accepted" && &(job.result) == "Accepted" {
                                performance_temp.score = job.score;
                                performance_temp.submission_time = job.created_time.clone();
                                performance_temp.job_id = job_cnt;
                            } 
                            else if &(job_list[performance_temp.job_id as usize].result) == "Accepted" && &(job.result) == "Accepted" {
                                if NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() <
                                NaiveDateTime::parse_from_str(&job.created_time, format).unwrap() {
                                    performance_temp.score = job.score;
                                    performance_temp.submission_time = job.created_time.clone();
                                    performance_temp.job_id = job_cnt;
                                }
                            }
                            else if &(job_list[performance_temp.job_id as usize].result) != "Accepted" && &(job.result) != "Accepted" {
                                if NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() <
                                NaiveDateTime::parse_from_str(&job.created_time, format).unwrap() {
                                    performance_temp.score = job.score;
                                    performance_temp.submission_time = job.created_time.clone();
                                    performance_temp.job_id = job_cnt;
                                }
                            }
                        }
                        //无竞争得分
                        else {
                            if NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() <
                            NaiveDateTime::parse_from_str(&job.created_time, format).unwrap() {
                                performance_temp.score = job.score;
                                performance_temp.submission_time = job.created_time.clone();
                                performance_temp.job_id = job_cnt;
                            }
                        }
                    }
                }
                job_cnt += 1;
            }
        }
    } 
    //scoring_rule == "highest"
    else {
        for user in &mut users_in_contest {
            let mut job_cnt = 0;
            for job in job_list.iter() {
                if Some(job.submission.user_id) == user.user_info.id && 
                user.performances.contains_key(&job.submission.problem_id) {
                    let performance_temp = user.performances.get_mut(&job.submission.problem_id).unwrap();
                    if performance_temp.if_did == false {
                        performance_temp.if_did = true;
                        performance_temp.score = job.score;
                        performance_temp.submission_time = job.created_time.clone();
                        performance_temp.submission_count += 1;
                        performance_temp.job_id = job_cnt;
                    } else {
                        performance_temp.submission_count += 1;
                        let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
                        //有竞争得分
                        if *(if_dyn.get(&job.submission.problem_id).unwrap()) == true {
                            if &(job_list[performance_temp.job_id as usize].result) != "Accepted" && &(job.result) == "Accepted" {
                                performance_temp.score = job.score;
                                performance_temp.submission_time = job.created_time.clone();
                                performance_temp.job_id = job_cnt;
                            } 
                            else if &(job_list[performance_temp.job_id as usize].result) == "Accepted" && &(job.result) == "Accepted" {
                                if NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() <
                                NaiveDateTime::parse_from_str(&job.created_time, format).unwrap() {
                                    performance_temp.score = job.score;
                                    performance_temp.submission_time = job.created_time.clone();
                                    performance_temp.job_id = job_cnt;
                                }
                            }
                            else if &(job_list[performance_temp.job_id as usize].result) != "Accepted" && &(job.result) != "Accepted" {
                                if job.score > performance_temp.score {
                                    performance_temp.score = job.score;
                                    performance_temp.submission_time = job.created_time.clone();
                                    performance_temp.job_id = job_cnt;
                                }
                                else if job.score == performance_temp.score
                                && NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() >
                                NaiveDateTime::parse_from_str(&job.created_time, format).unwrap() {
                                    performance_temp.score = job.score;
                                    performance_temp.submission_time = job.created_time.clone();
                                    performance_temp.job_id = job_cnt;
                                }
                            }
                        }
                        //无竞争得分
                        else {
                            if job.score > performance_temp.score {
                                performance_temp.score = job.score;
                                performance_temp.submission_time = job.created_time.clone();
                                performance_temp.job_id = job_cnt;
                            }
                            else if job.score == performance_temp.score
                            && NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() >
                            NaiveDateTime::parse_from_str(&job.created_time, format).unwrap() {
                                performance_temp.score = job.score;
                                performance_temp.submission_time = job.created_time.clone();
                                performance_temp.job_id = job_cnt;
                            }
                        }
                    }
                }
                job_cnt += 1;
            }
        }
    }
    //以下为构建 prob_info
    let mut prob_info: HashMap<i32, ProbInfo> = HashMap::new();
    if contest_id == 0 {
        for problem in &setting.problems {
            let mut scores: Vec<f64> = Vec::new();
            for case in &problem.cases {
                scores.push(case.score);
            }
            if problem.misc.dynamic_ranking_ratio == None {
                prob_info.entry(problem.id).or_insert(ProbInfo { min_time: Vec::new(), ratio: None, full_score: scores });
            }
            else {
                prob_info.entry(problem.id).or_insert(ProbInfo { min_time: Vec::new(), ratio: problem.misc.dynamic_ranking_ratio, full_score: scores });
            }
        }
        //注意：如果一个“竞争得分”的题没有一个 ac 的 job，那么虽然 min_time 为空 Vec，但之后也不会被调用
        //以下为求 prob_info 的 info 中的 min_time
        for job in job_list.iter() {
            let temp_prob_info = prob_info.get_mut(&(job.submission.problem_id)).unwrap();
            if &(job.result) == "Accepted" && temp_prob_info.ratio != None {
                if temp_prob_info.min_time.is_empty() == true {
                    for case_id in 1..job.cases.len() {
                        temp_prob_info.min_time.push(job.cases[case_id].time);
                    }
                } 
                else {
                    for i in 1..job.cases.len() {
                        if temp_prob_info.min_time[i - 1] > job.cases[i].time {
                            temp_prob_info.min_time[i - 1] = job.cases[i].time;
                        }
                    }
                }
            }
        }
    }
    else {
        for problem_contest_id in &contest_list[contest_id as usize - 1].problem_ids {
            for problem in &setting.problems {
                if problem_contest_id == &problem.id {
                    let mut scores: Vec<f64> = Vec::new();
                    for case in &problem.cases {
                        scores.push(case.score);
                    }
                    if problem.misc.dynamic_ranking_ratio == None {
                        prob_info.entry(problem.id).or_insert(ProbInfo { min_time: Vec::new(), ratio: None, full_score: scores });
                    }
                    else {
                        prob_info.entry(problem.id).or_insert(ProbInfo { min_time: Vec::new(), ratio: problem.misc.dynamic_ranking_ratio, full_score: scores });
                    }
                }
            }
        } 
        //注意：如果一个“竞争得分”的题没有一个 ac 的 job，那么虽然 min_time 为空 Vec，但之后也不会被调用
        for job in job_list.iter() {
            if prob_info.contains_key(&(job.submission.problem_id)) == true {
                let temp_prob_info = prob_info.get_mut(&(job.submission.problem_id)).unwrap();
                if &(job.result) == "Accepted" && temp_prob_info.ratio != None && job.submission.contest_id == contest_id {
                    if temp_prob_info.min_time.is_empty() == true {
                        for case_id in 1..job.cases.len() {
                            temp_prob_info.min_time.push(job.cases[case_id].time);
                        }
                    } 
                    else {
                        for i in 1..job.cases.len() {
                            if temp_prob_info.min_time[i - 1] > job.cases[i].time {
                                temp_prob_info.min_time[i - 1] = job.cases[i].time;
                            }
                        }
                    }
                }
            }
        }
    }
    //算竞争得分
    for user in &mut users_in_contest {
        for (prob_id, performance) in  &mut user.performances {
            if performance.if_did == true && job_list.len() > 0 && &(job_list[performance.job_id as usize].result) == "Accepted" {
                let temp_prob_info = prob_info.get(prob_id).unwrap();
                match temp_prob_info.ratio {
                    None => {}
                    Some(ratio) => {
                        for i in 1..job_list[performance.job_id as usize].cases.len() {
                            performance.score += temp_prob_info.full_score[i - 1] 
                            * ratio * (temp_prob_info.min_time[i - 1] as f64) / (job_list[performance.job_id as usize].cases[i].time as f64);
                        }
                    }
                }
            }
        }
    }
    //加总分
    for user in &mut users_in_contest {
        for (_prob_id, performance_temp) in &user.performances {
            if performance_temp.if_did == true {
                user.total_score += performance_temp.score;
            }
        }
    }
    //排序并且更新 user_in_contest 参数状态；最后按分数排序；先排序再排名
    if &tie_breaker == "submission_time" {
        //更新 user.submission_time
        for user in &mut users_in_contest {
            for (_prob_id, performance_temp) in &user.performances {
                if &user.submssion_time == "-1" && performance_temp.if_did == true {
                    user.submssion_time = performance_temp.submission_time.clone();
                } 
                else if performance_temp.if_did == true {
                    let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
                    if NaiveDateTime::parse_from_str(&performance_temp.submission_time, format).unwrap() >
                    NaiveDateTime::parse_from_str(&user.submssion_time, format).unwrap() {
                        user.submssion_time = performance_temp.submission_time.clone();
                    }
                }
            }
        }
        //Inspired from GPT
        //自定义比较时间函数
        fn cmp_sub_time(a: &str, b: &str) -> std::cmp::Ordering {
            let format = "%Y-%m-%dT%H:%M:%S%.3fZ";
            if a == "-1" && b != "-1" {
                return std::cmp::Ordering::Greater;
            } 
            else if a != "-1" && b == "-1" {
                return std::cmp::Ordering::Less;
            } 
            else if a == "-1" && b == "-1" {
                return std::cmp::Ordering::Equal;
            } 
            else {
                if NaiveDateTime::parse_from_str(a, format).unwrap() >
                NaiveDateTime::parse_from_str(b, format).unwrap() {
                    return std::cmp::Ordering::Greater;
                } 
                else if NaiveDateTime::parse_from_str(a, format).unwrap() <
                NaiveDateTime::parse_from_str(b, format).unwrap() {
                    return std::cmp::Ordering::Less;
                } else {
                    return std::cmp::Ordering::Equal;
                }
            }
        }//（思路：最重要的排序指标最后排序）
        users_in_contest.sort_by(|a, b| a.user_info.id.unwrap().cmp(&b.user_info.id.unwrap()));
        users_in_contest.sort_by(|a, b| cmp_sub_time(&a.submssion_time, &b.submssion_time));
        users_in_contest.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap()); 
        //排名
        users_in_contest[0].rank = 1;
        if users_in_contest.len() > 0 {
            for i in 1..users_in_contest.len() {
                if users_in_contest[i].total_score == users_in_contest[i - 1].total_score
                && cmp_sub_time(&users_in_contest[i].submssion_time, &users_in_contest[i - 1].submssion_time) == std::cmp::Ordering::Equal {
                    users_in_contest[i].rank = users_in_contest[i - 1].rank;
                } else {
                    users_in_contest[i].rank = (i + 1) as i32;
                }
            }
        }
    }
    else if &tie_breaker == "submission_count" {
        //更新 user.submission_count
        for user in &mut users_in_contest {
            for (_prob_id, performance_temp) in &user.performances {
                user.total_submission_count += performance_temp.submission_count;
            }
        }
        users_in_contest.sort_by(|a, b| a.user_info.id.unwrap().cmp(&b.user_info.id.unwrap()));
        users_in_contest.sort_by(|a, b| a.total_submission_count.cmp(&b.total_submission_count));
        users_in_contest.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap()); 
        //排名
        users_in_contest[0].rank = 1;
        if users_in_contest.len() > 0 {
            for i in 1..users_in_contest.len() {
                if users_in_contest[i].total_score == users_in_contest[i - 1].total_score
                && &users_in_contest[i].total_submission_count == &users_in_contest[i - 1].total_submission_count {
                    users_in_contest[i].rank = users_in_contest[i - 1].rank;
                } else {
                    users_in_contest[i].rank = (i + 1) as i32;
                }
            }
        }
    }
    else if &tie_breaker == "user_id" {
        users_in_contest.sort_by(|a, b| a.user_info.id.unwrap().cmp(&b.user_info.id.unwrap()));
        users_in_contest.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap()); 
        //排名
        users_in_contest[0].rank = 1;
        if users_in_contest.len() > 0 {
            for i in 1..users_in_contest.len() {
                if users_in_contest[i].total_score == users_in_contest[i - 1].total_score
                && &users_in_contest[i].user_info.id == &users_in_contest[i - 1].user_info.id {
                    users_in_contest[i].rank = users_in_contest[i - 1].rank;
                } else {
                    users_in_contest[i].rank = (i + 1) as i32;
                }
            }
        }
    } 
    else {
            users_in_contest.sort_by(|a, b| a.user_info.id.unwrap().cmp(&b.user_info.id.unwrap()));
            users_in_contest.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap());
            //排名
            users_in_contest[0].rank = 1;
            if users_in_contest.len() > 0 {
                for i in 1..users_in_contest.len() {
                    if users_in_contest[i].total_score == users_in_contest[i - 1].total_score{
                        users_in_contest[i].rank = users_in_contest[i - 1].rank;
                    } else {
                        users_in_contest[i].rank = (i + 1) as i32;
                    }
                }
            }
        }
    //生成响应 json
    if contest_id == 0 {
        let mut ranklist: Vec<UserInContestJson> = Vec::new();
        for user in users_in_contest {
            let mut temp_scores: Vec<f64> = Vec::new();
            let mut performances_vec: Vec<(i32, Performance)> = user.performances.into_iter().collect();
            performances_vec.sort_by(|a, b| a.0.cmp(&b.0));
            for i in performances_vec {
                temp_scores.push(i.1.score);
            }
            let user_json: UserInContestJson = UserInContestJson { user: user.user_info, rank: user.rank, scores: temp_scores };
            ranklist.push(user_json);
        }
        return HttpResponse::Ok().json(ranklist);
    }
    //单个比赛排行
    else {
        let mut ranklist: Vec<UserInContestJson> = Vec::new();
        for user in users_in_contest {
            let mut temp_scores: Vec<f64> = Vec::new();
            for i in &contest_list[contest_id as usize - 1].problem_ids {
                temp_scores.push(user.performances.get(i).unwrap().score);
            }
            let user_json: UserInContestJson = UserInContestJson { user: user.user_info, rank: user.rank, scores: temp_scores };
            ranklist.push(user_json);
        }
        return HttpResponse::Ok().json(ranklist);
    }
}
#[post("/contests")]
async fn post_contests(mut body: web::Json<Contest>, setting: web::Data<Setting>, req: HttpRequest, 
secret_key: web::Data<DecodingKey>, if_token: web::Data<bool>) -> impl Responder {
    let mut check_problem_id = 1;
    let mut invalid_problem_id = -1;
    let mut check_user_id = 1;
    let mut invalid_user_id = -1;
    let user_list = USERS.lock().unwrap();
    let mut contest_list = CONTESTS.lock().unwrap();
    //鉴权
    if *if_token == true.into() {
        let deco_result = decoding(req.clone(), secret_key);
        if deco_result == None {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else if deco_result != Some(String::from("Author")) {
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Only Author have the right."),
            });
        }
    }
    //判断 problem_id 是否都存在
    for i in &body.problem_ids {
        let mut inner_check = 0;
        for j in &setting.problems {
            if i == &j.id {
                inner_check = 1;
                break;
            }
        }
        if inner_check == 0 {
            invalid_problem_id = *i;
            check_problem_id = 0;
            break;
        }
    }
    if check_problem_id == 0 {
        let mut message_str = String::from("Problem ");
        message_str.push_str(&invalid_problem_id.to_string());
        message_str.push_str(" not found.");
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : message_str,
        });
    }
    //判断 problem_id 是否重复
    let hash_set: HashSet<&i32> = body.problem_ids.iter().collect();
    if hash_set.len() != body.problem_ids.len() {
        return HttpResponse::BadRequest().json(Error {
            code : 1,
            reason : String::from("ERR_INVALID_ARGUMENT"), 
            message : String::from("Invalid argument problem_ids"),
        });
    }
    //判断 user_id 是否都存在
    for i in &body.user_ids {
        let mut inner_check = 0;
        for j in user_list.iter() {
            if Some(*i) == j.id {
                inner_check = 1;
                break;
            }
        }
        if inner_check == 0 {
            invalid_user_id = *i;
            check_user_id = 0;
            break;
        }
    }
    if check_user_id == 0 {
        let mut message_str = String::from("User ");
        message_str.push_str(&invalid_user_id.to_string());
        message_str.push_str(" not found.");
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : message_str,
        });
    }
    //判断 user_id 是否重复
    let hash_set: HashSet<&i32> = body.user_ids.iter().collect();
    if hash_set.len() != body.user_ids.len() {
        return HttpResponse::BadRequest().json(Error {
            code : 1,
            reason : String::from("ERR_INVALID_ARGUMENT"), 
            message : String::from("Invalid argument user_ids"),
        });
    }
    //用正则表达式判断时间是否合法
    let pattern = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z";
    let re = Regex::new(pattern).unwrap();
    if re.is_match(&body.from) == false {
        return HttpResponse::BadRequest().json(Error {
            code : 1,
            reason : String::from("ERR_INVALID_ARGUMENT"), 
            message : String::from("Invalid argument from"),
        });
    }
    if re.is_match(&body.to) == false {
        return HttpResponse::BadRequest().json(Error {
            code : 1,
            reason : String::from("ERR_INVALID_ARGUMENT"), 
            message : String::from("Invalid argument to"),
        });
    }
    match body.id {
        None => {
            body.id = Some((contest_list.len() + 1) as i32);
            contest_list.push(body.clone());
            return HttpResponse::Ok().json(body);
        }
        Some(id) => {
            if id > contest_list.len() as i32 {
                let mut message_str = String::from("Contest ");
                message_str.push_str(&id.to_string());
                message_str.push_str(" not found.");
                save_contest_list((*contest_list.clone()).to_vec(), "contest_list_saved.json");
                return HttpResponse::NotFound().json(Error {
                    code : 3,
                    reason : String::from("ERR_NOT_FOUND"), 
                    message : message_str,
                });
            }
            if id == 0 {
                return HttpResponse::BadRequest().json(Error {
                    code : 1,
                    reason : String::from("ERR_INVALID_ARGUMENT"), 
                    message : String::from("Invalid contest id"),
                });
            }
            contest_list[(id - 1) as usize] = body.clone();
            save_contest_list((*contest_list.clone()).to_vec(), "contest_list_saved.json");
            return HttpResponse::Ok().json(body);
        }
    }
}
#[get("/contests")]
async fn get_contests() -> impl Responder {
    let contest_list: std::sync::MutexGuard<'_, Vec<Contest>> = CONTESTS.lock().unwrap();
    return HttpResponse::Ok().json(contest_list.clone());
}
#[get("/contests/{contestID}")]
async fn get_contests_id(contest_id_arg: web::Path<String>) -> impl Responder {
    let contest_id_str: String = contest_id_arg.to_string();
    let contest_id: i32 = contest_id_str.parse().unwrap();
    let contest_list = CONTESTS.lock().unwrap();
    if contest_id > contest_list.len() as i32 {
        let mut message_str = String::from("Contest ");
        message_str.push_str(&contest_id.to_string());
        message_str.push_str(" not found.");
        return HttpResponse::NotFound().json(Error {
            code : 3,
            reason : String::from("ERR_NOT_FOUND"), 
            message : message_str,
        });
    }
    if contest_id == 0 {
        return HttpResponse::BadRequest().json(Error {
            code : 1,
            reason : String::from("ERR_INVALID_ARGUMENT"), 
            message : String::from("Invalid contest id"),
        });
    }
    return HttpResponse::Ok().json(contest_list[contest_id as usize -1].clone());
}
#[post("/register")]
async fn post_register(req: HttpRequest, user_plus: web::Json<UserPlus>, if_token: web::Data<bool>,
secret_key: web::Data<DecodingKey>) -> impl Responder {
    if *if_token == true.into() {
        if decoding(req, secret_key).is_some() == true {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log out first."),
            })
        } 
        //以下为注册过程
        else {
            let mut user_list = USERS.lock().unwrap();
            let mut user_plus_list = USER_PLUS_LIST.lock().unwrap();
            //判断重名
            for i in user_list.iter() {
                if i.name == user_plus.name {
                    let mut message_str = String::from("User name '");
                    message_str.push_str(&i.name);
                    message_str.push_str("' already exists.");
                    return HttpResponse::BadRequest().json(Error {
                        code : 1,
                        reason : String::from("ERR_INVALID_ARGUMENT"), 
                        message : message_str,
                    });
                }
            }
            let new_user = User { 
                id: Some(user_list.len() as i32), 
                name: user_plus.name.clone()
            };
            user_list.push(new_user.clone());
            let mut new_user_plus = UserPlus { 
                id: Some(user_plus_list.len() as i32), 
                name: user_plus.name.clone(),
                key: bcrypt::hash(user_plus.key.clone(), 11).unwrap(),
                identity: user_plus.identity.clone()
            };
            if new_user_plus.identity.is_none() == true {
                new_user_plus.identity = Some(String::from("CommonUser"));
            }
            user_plus_list.push(new_user_plus);
            save_user_list((*user_list.clone()).to_vec(), "user_list_saved.json");
            save_user_plus_list((*user_plus_list.clone()).to_vec(), "user_plus_list_saved.json");
            return HttpResponse::Ok().json(new_user);
        }
    }
    else {
        return HttpResponse::BadRequest().json(Error {
            code : 7,
            reason : String::from("ERR_INVALID_TOKENT"), 
            message: String::from("Not in user-management mode."),
        })
    }
}
#[post("/login")]
async fn post_login(req: HttpRequest, user_plus: web::Json<UserPlus>, if_token: web::Data<bool>, 
secret_key: web::Data<DecodingKey>, encoding_key: web::Data<EncodingKey>) -> impl Responder {
    if *if_token == true.into() {
        if decoding(req, secret_key).is_some() == true {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log out first"),
            })
        } 
        //以下为登录过程
        else {
            let user_plus_list = USER_PLUS_LIST.lock().unwrap();
            for user_plus_saved in user_plus_list.iter() {
                if user_plus_saved.name == user_plus.name {
                    if let Ok(true) = bcrypt::verify(user_plus.key.clone(), &user_plus_saved.key) {
                        //Inspired from GPT
                        //由 user 信息生成 token，并给 token 设置过期时间
                        let now = Utc::now();
                        let expiration = now + chrono::Duration::hours(5);
                        let claims = UserPlusClaim {
                            id: user_plus_saved.id.clone(),
                            name: user_plus_saved.name.clone(),
                            identity: user_plus_saved.identity.clone(),
                            exp: expiration.timestamp() as usize,
                        };
                        let mut token: String = String::from("Bearer ");
                        token.push_str(&(encode(&Header::new(Algorithm::HS256), &claims, &encoding_key).unwrap()));
                        return HttpResponse::Ok().json(token);
                    }
                    //密码错误
                    else {
                        return HttpResponse::BadRequest().json(Error {
                            code : 8,
                            reason : String::from("ERR_INVALID_ARGUMENT"), 
                            message: String::from("Wrong user_name or wrong key"),
                        });
                    }
                }
            }
            //用户不存在
            return HttpResponse::BadRequest().json(Error {
                code : 8,
                reason : String::from("ERR_INVALID_ARGUMENT"), 
                message: String::from("Wrong user_name or wrong key"),
            });
        }
    }
    else {
        return HttpResponse::BadRequest().json(Error {
            code : 7,
            reason : String::from("ERR_INVALID_TOKENT"), 
            message: String::from("Not in user-management mode."),
        })
    }
}
#[post("/logout")]
async fn post_logout(req: HttpRequest, if_token: web::Data<bool>, secret_key: web::Data<DecodingKey>) -> impl Responder {
    if *if_token == true.into() {
        if decoding(req.clone(), secret_key).is_none() == true {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Already log out"),
            });
        } 
        //以下为登出过程
        else {
            let mut token = String::new();
            for (key, value) in req.headers() {
                if &(key.to_string()) == "authorization" {
                    token = value.to_str().unwrap().to_string();
                }
            }
            let mut blacklist = BLACKLIST.lock().unwrap();
            blacklist.push(token);
            return HttpResponse::Ok().json(String::from("Log out Successfully"));
        }
    }
    else {
        return HttpResponse::BadRequest().json(Error {
            code : 7,
            reason : String::from("ERR_INVALID_TOKENT"), 
            message: String::from("Not in user-management mode."),
        });
    }
}
#[post("/changename")]
async fn post_changename(req: HttpRequest, if_token: web::Data<bool>, secret_key: web::Data<DecodingKey>, 
mut change_name: web::Json<ChangeName>) -> impl Responder {
    if *if_token == true.into() {
        let mut token = String::new();
        for (key, value) in req.headers() {
            if &(key.to_string()) == "authorization" {
                token = value.to_str().unwrap().to_string();
            }
        }
        if BLACKLIST.lock().unwrap().contains(&token) == true {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        let parts: Vec<&str> = token.split_whitespace().collect();
        if parts.len() != 2 {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else {
            token = parts[1].to_string();
        }
        match decode::<UserPlusClaim>(&token, &secret_key, &Validation::new(Algorithm::HS256)) {
            Ok (data) => {
                if BLACKLIST.lock().unwrap().contains(&token) == true {
                    return HttpResponse::BadRequest().json(Error {
                        code : 7,
                        reason : String::from("ERR_INVALID_TOKENT"), 
                        message: String::from("Please log in first."),
                    });
                }
                //以下为改名过程
                else {
                    //判断重名
                    let mut user_list = USERS.lock().unwrap();
                    let mut user_plus_list = USER_PLUS_LIST.lock().unwrap();
                    for user_saved in user_list.iter() {
                        if user_saved.name == change_name.after && data.claims.name != change_name.after {
                            let mut message_str = String::from("User name '");
                            message_str.push_str(&user_saved.name);
                            message_str.push_str("' already exists.");
                            return HttpResponse::BadRequest().json(Error {
                                code : 1,
                                reason : String::from("ERR_INVALID_ARGUMENT"), 
                                message : message_str,
                            });
                        }
                    }
                    change_name.before = Some(user_list[data.claims.id.unwrap() as usize].name.clone());
                    user_list[data.claims.id.unwrap() as usize].name = change_name.after.clone();
                    user_plus_list[data.claims.id.unwrap() as usize].name = change_name.after.clone();
                    save_user_list((*user_list.clone()).to_vec(), "user_list_saved.json");
                    save_user_plus_list((*user_plus_list.clone()).to_vec(), "user_plus_list_saved.json");
                    return HttpResponse::Ok().json(change_name.clone());
                }
            }
            Err(_err) => {
                return HttpResponse::BadRequest().json(Error {
                    code : 7,
                    reason : String::from("ERR_INVALID_TOKENT"), 
                    message: String::from("Please log in first."),
                });
            }
        }
        
    }
    else {
        return HttpResponse::BadRequest().json(Error {
            code : 7,
            reason : String::from("ERR_INVALID_TOKENT"), 
            message: String::from("Not in user-management mode."),
        });
    }
}
#[post("/changenames")]
async fn post_changenames(req: HttpRequest, if_token: web::Data<bool>, secret_key: web::Data<DecodingKey>, 
change_name: web::Json<ChangeName>) -> impl Responder {
    if *if_token == true.into() {
        let mut token = String::new();
        for (key, value) in req.headers() {
            if &(key.to_string()) == "authorization" {
                token = value.to_str().unwrap().to_string();
            }
        }
        if BLACKLIST.lock().unwrap().contains(&token) == true {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        let parts: Vec<&str> = token.split_whitespace().collect();
        if parts.len() != 2 {
            return HttpResponse::BadRequest().json(Error {
                code : 7,
                reason : String::from("ERR_INVALID_TOKENT"), 
                message: String::from("Please log in first."),
            });
        }
        else {
            token = parts[1].to_string();
        }
        match decode::<UserPlusClaim>(&token, &secret_key, &Validation::new(Algorithm::HS256)) {
            Ok (data) => {
                if &data.claims.identity.unwrap() != "Administrator" {
                    return HttpResponse::BadRequest().json(Error {
                        code : 8,
                        reason : String::from("ERR_INVALID_TOKENT"), 
                        message: String::from("Only Administrator have the right."),
                    });
                }
                //以下为改名过程
                else {
                    //判断重名
                    let mut user_list = USERS.lock().unwrap();
                    let mut user_plus_list = USER_PLUS_LIST.lock().unwrap();
                    for user_saved in user_list.iter() {
                        if user_saved.name == change_name.after && change_name.before != Some(change_name.after.clone()) {
                            let mut message_str = String::from("User name '");
                            message_str.push_str(&user_saved.name);
                            message_str.push_str("' already exists.");
                            return HttpResponse::BadRequest().json(Error {
                                code : 1,
                                reason : String::from("ERR_INVALID_ARGUMENT"), 
                                message : message_str,
                            });
                        }
                    }
                    if change_name.before.is_none() == true {
                        return HttpResponse::BadRequest().json(Error {
                            code : 1,
                            reason : String::from("ERR_INVALID_ARGUMENT"), 
                            message : String::from("Invalid argument before."),
                        });
                    }
                    for i in 0..user_list.len() {
                        if user_list[i].name == change_name.before.clone().unwrap() {
                            user_list[i].name = change_name.after.clone();
                            user_plus_list[i].name = change_name.after.clone();
                            break;
                        }
                    }
                    save_user_list((*user_list.clone()).to_vec(), "user_list_saved.json");
                    save_user_plus_list((*user_plus_list.clone()).to_vec(), "user_plus_list_saved.json");
                    return HttpResponse::Ok().json(change_name.clone());
                }
            }
            Err(_err) => {
                return HttpResponse::BadRequest().json(Error {
                    code : 7,
                    reason : String::from("ERR_INVALID_TOKENT"), 
                    message: String::from("Please log in first."),
                });
            }
        }
        
    }
    else {
        return HttpResponse::BadRequest().json(Error {
            code : 7,
            reason : String::from("ERR_INVALID_TOKENT"), 
            message: String::from("Not in user-management mode."),
        });
    }
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut meet_word_argument = 0;
    let mut wait_config = 0;
    let mut if_token = false;
    let mut setting: Setting = Setting {
        server: Server { bind_address: Some(String::from("127.0.0.1")), bind_port: Some(12345) },
        problems: Vec::new(),
        languages: Vec::new(),
    };
    let mut config_file_name: String;
    //解析 config 文件
    for arg in std::env::args() {
        let arg = arg.trim().to_string();
        if meet_word_argument == 1 {
            if wait_config == 1 {
                config_file_name = arg;
                if let Ok(mut f) = std::fs::File::open(config_file_name.clone()) {
                    let mut temp_string = String::new();
                    f.read_to_string(&mut temp_string)?;
                    if let Err(_) = serde_json::from_str::<Value>(&temp_string) {
                        panic! ("json wrong");
                    } else {
                        setting = serde_json::from_str(&temp_string)?;
                    }
                }
                meet_word_argument = 0;
            }
        }
        else if arg == "-c" || arg == "--config" {
            wait_config = 1;
            meet_word_argument = 1;
        }
        else if arg == "-t" || arg == "--token" {
            if_token = true;
        }
        else if arg == "-f" || arg == "--flush-data" {
            let _ = std::fs::remove_file("job_list_saved.json");
            let _ = std::fs::remove_file("contest_list_saved.json");
            let _ = std::fs::remove_file("user_list_saved.json");
            let _ = std::fs::remove_file("user_plus_list_saved.json");
        }
    }
    
    if meet_word_argument == 1 {
        panic!("Command word missing");
    }
    if setting.server.bind_address.is_none() == true {
        setting.server.bind_address = Some(String::from("127.0.0.1"));
    }
    if setting.server.bind_port.is_none() == true {
        setting.server.bind_port = Some(12345);
    }
    let setting_address = setting.server.bind_address.clone().unwrap();
    let setting_port = setting.server.bind_port.clone().unwrap();
    //Inspired from GPT
    //生成密钥
    let secret_key_en: EncodingKey;
    let secret_key_de: DecodingKey;
    let mut secret_key_str: String = String::new();
    let mut temp_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut temp_key);
    for i in temp_key {
        secret_key_str.push_str(&(i.to_string()));
    }
    secret_key_en = EncodingKey::from_secret(secret_key_str.as_bytes());
    secret_key_de = DecodingKey::from_secret(secret_key_str.as_bytes());
    //如果有存档文件则读取存档
    if let Ok(mut f) = std::fs::File::open("job_list_saved.json") {
        let mut job_list = JOB_LIST.lock().unwrap();
        let mut json_str = String::new();
        f.read_to_string(&mut json_str)?;
        *job_list = serde_json::from_str(&json_str).unwrap();
    }
    if let Ok(mut f) = std::fs::File::open("user_list_saved.json") {
        let mut user_list = USERS.lock().unwrap();
        let mut json_str = String::new();
        f.read_to_string(&mut json_str)?;
        *user_list = serde_json::from_str(&json_str).unwrap();
    }
    else {
        //创建 root 用户
        let new_user: User = User {id: Some(0), name: String::from("root")};
        USERS.lock().unwrap().push(new_user);
    }
    if let Ok(mut f) = std::fs::File::open("user_plus_list_saved.json") {
        let mut user_plus_list = USER_PLUS_LIST.lock().unwrap();
        let mut json_str = String::new();
        f.read_to_string(&mut json_str)?;
        *user_plus_list = serde_json::from_str(&json_str).unwrap();
    }
    else {
        //创建 root 用户
        let new_user: UserPlus = UserPlus { id: Some(0), name: String::from("root"), key: String::new(), identity: Some(String::from("Administrator")) };
        USER_PLUS_LIST.lock().unwrap().push(new_user);
    }
    if let Ok(mut f) = std::fs::File::open("contest_list_saved.json") {
        let mut contest_list = CONTESTS.lock().unwrap();
        let mut json_str = String::new();
        f.read_to_string(&mut json_str)?;
        *contest_list = serde_json::from_str(&json_str).unwrap();
    }
    //开始监听
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(setting.clone()))
            .app_data(web::Data::new(if_token))
            .app_data(web::Data::new(secret_key_de.clone()))
            .app_data(web::Data::new(secret_key_en.clone()))
            .wrap(Logger::default())
            .route("/hello", web::get().to(|| async { "Hello World!" }))
            .service(post_jobs)
            .service(get_jobs)
            .service(get_job_id)
            .service(put_job_id)
            .service(post_users)
            .service(get_users)
            .service(get_contests_id_ranklist)
            .service(post_contests)
            .service(get_contests)
            .service(get_contests_id)
            .service(post_register)
            .service(post_login)
            .service(post_logout)
            .service(post_changename)
            .service(post_changenames)
            .service(exit)
    })
    .bind((setting_address, setting_port))?
    .run()
    .await
    
}
//函数：判断状态是否为登入，未登入则返回 None，已经登入则返回 Identity
fn decoding(req: HttpRequest, secret_key: web::Data<DecodingKey>) -> Option<String> {
    let mut token = String::new();
    for (key, value) in req.headers() {
        if &(key.to_string()) == "authorization" {
            token = value.to_str().unwrap().to_string();
        }
    }
    if BLACKLIST.lock().unwrap().contains(&token) == true {
        return None;
    }
    let parts: Vec<&str> = token.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    else {
        token = parts[1].to_string();
    }
    match decode::<UserPlusClaim>(&token, &secret_key, &Validation::new(Algorithm::HS256)) {
        Ok (data) => {
            return data.claims.identity.clone();
        }
        Err(_err) => {
            return None;
        }
    }
}
//保存 JOB_LIST
fn save_job_list(job_list: Vec<JsonResponse>, file_path: &str) {
    let mut f = File::create(file_path).unwrap();
    f.write_all(serde_json::to_string(&job_list).unwrap().as_bytes()).unwrap();
}
//保存 USERS
fn save_user_list(user_list: Vec<User>, file_path: &str) {
    let mut f = File::create(file_path).unwrap();
    f.write_all(serde_json::to_string(&user_list).unwrap().as_bytes()).unwrap();
}
//保存 USER_PLUS_LIST
fn save_user_plus_list(user_plus_list: Vec<UserPlus>, file_path: &str) {
    let mut f = File::create(file_path).unwrap();
    f.write_all(serde_json::to_string(&user_plus_list).unwrap().as_bytes()).unwrap();
}
//保存 CONTESTS
fn save_contest_list(contest_list: Vec<Contest>, file_path: &str) {
    let mut f = File::create(file_path).unwrap();
    f.write_all(serde_json::to_string(&contest_list).unwrap().as_bytes()).unwrap();
}
