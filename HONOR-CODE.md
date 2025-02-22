# Honor Code
1、关于如何将%Y-%m-%dT%H:%M:%S%.3fZ 类型的字符串转化为时间，我询问了 GPT“如何从%Y-%m-%dT%H:%M:%S%.3fZ 格式的 String 转换为时间”，答案：你可以按照以下步骤将 %Y-%m-%dT%H:%M:%S%.3fZ 格式的时间字符串转换为 DateTime 对象
```Rust
use chrono::{DateTime, TimeZone, Utc, NaiveDateTime};

fn main() {
    // 输入的时间字符串
    let time_str = "2023-07-20T12:34:56.789Z";

    // 将时间字符串解析为 DateTime 对象
    let dt = parse_datetime(time_str).expect("Invalid datetime format");

    // 现在你可以使用 dt 进行其他操作
    println!("Parsed DateTime: {:?}", dt);
}

// 解析时间字符串并返回 DateTime 对象
fn parse_datetime(time_str: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    // 解析成 NaiveDateTime
    let naive_dt = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M:%S%.3fZ")?;

    // 将 NaiveDateTime 转换为 DateTime<Utc>
    Ok(DateTime::<Utc>::from_utc(naive_dt, Utc))
}

```
启发了我我怎么将该格式的字符串转化为时间

2、我在 2023-07-14 参考了 Rust 官方在 https://docs.rs/wait-timeout/latest/wait_timeout/编写的代码，引入了 main.rs

3、我在 2023-07-14 参考了 Rokit 在 https://stackoverflow.com/questions/54406029/how-can-i-parse-query-strings-in-actix-web 的代码，引入了 main.rs

4、关于“如何判断字符串是否符合%Y-%m-%dT%H:%M:%S%.3fZ 格式”，我询问了 GPT“怎么符串是否符合%Y-%m-%dT%H:%M:%S%.3fZ 格式”，答案：你可以使用正则表达式来判断一个字符串是否符合特定的格式。对于"%Y-%m-%dT%H:%M:%S%.3fZ"这种格式，其中包含日期、时间和毫秒部分，我们可以使用正则表达式进行匹配。
```Rust
use regex::Regex;

fn is_valid_datetime_format(input: &str) -> bool {
    // 定义日期时间的正则表达式
    let datetime_regex = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").unwrap();
    
    // 使用正则表达式匹配字符串
    datetime_regex.is_match(input)
}

fn main() {
    let input_str = "2023-07-20T12:34:56.789Z";
    if is_valid_datetime_format(input_str) {
        println!("Valid datetime format.");
    } else {
        println!("Invalid datetime format.");
    }
}

```
这启发了我用正则表达式判断字符串是否符合%Y-%m-%dT%H:%M:%S%.3fZ 格式

5、关于“如何自定义比较函数”，我询问了 GPT 怎么“怎么自定义比较大小的函数”，回答：自定义 PartialOrd 比较函数：
PartialOrd trait 用于定义部分排序（不完全排序）的类型。它要求实现 partial_cmp 方法，该方法返回一个 Option<Ordering>，表示两个值的排序关系。
以下是一个自定义比较函数的示例，我们将创建一个简单的 Point 结构体，表示二维平面上的点，并为其实现 PartialOrd：
```Rust
use std::cmp::Ordering;

#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // 自定义比较规则：先比较x坐标，如果相同再比较y坐标
        if self.x != other.x {
            self.x.partial_cmp(&other.x)
        } else {
            self.y.partial_cmp(&other.y)
        }
    }
}

fn main() {
    let point1 = Point { x: 2, y: 5 };
    let point2 = Point { x: 1, y: 8 };
    let point3 = Point { x: 2, y: 3 };

    println!("point1 > point2: {:?}", point1 > point2); // Output: true
    println!("point1 < point3: {:?}", point1 < point3); // Output: false
}

```
这启发我自定义比较时间的函数

6、关于“怎么给 Token 添加过期时间”，我询问了 GPT“如何给 token 添加 expiration”，回答：要设置过期时间，您可以使用 JWT 库提供的方法将 exp 声明添加到令牌中。下面是示例代码：
```Rust
let now = Utc::now();
let expiration = now + Duration::hours(2);

let claims = UserClaims {
    id: new_user.id,
    name: new_user.name.clone(),
    key: new_user.key.clone(),
    identity: new_user.identity,
    exp: expiration.timestamp() as usize, // 设置过期时间
};

let secret_key_en = EncodingKey::from_secret(secret_key); // 假设 secret_key 是您的密钥

let test_token = encode(&Header::new(Algorithm::HS256), &claims, &secret_key_en).unwrap();
```
这启发我为 token 设置时间戳

7、关于怎么生成 token 所需的密钥，我询问了 GPT“怎么生成密钥 EncodingKey”，回答：在 Rust 中使用 jsonwebtoken 库来生成密钥 EncodingKey，您可以使用 jsonwebtoken::EncodingKey::from_secret 方法。

首先，您需要使用一个密钥来创建 EncodingKey，密钥的类型可以是 Vec<u8> 或者是 AsRef<[u8]>。在以下示例中，我们假设使用一个字符串作为密钥，然后将其转换为 Vec<u8>。
```Rust
use jsonwebtoken::{EncodingKey, Algorithm};

fn main() {
    let secret_key_str = "my_secret_key"; // 这里使用自定义的密钥字符串，您可以根据实际需求替换为自己的密钥

    // 将字符串转换为字节数组（Vec<u8>）
    let secret_key: Vec<u8> = secret_key_str.as_bytes().to_vec();

    // 通过密钥创建 EncodingKey
    let encoding_key = EncodingKey::from_secret(&secret_key);

    // 现在您可以将 encoding_key 用于加密生成令牌
    // 例如：encode(&Header::new(Algorithm::HS256), &claims, &encoding_key)
}

```
这启发我生成 EncodingKey 和 DecodingKey