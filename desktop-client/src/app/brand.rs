#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCopyTone {
    Launching,
    RedeemSuccess,
    BillingDue,
    Retry,
    Cleanup,
}

pub const fn product_name() -> &'static str {
    "一键开整"
}

pub const fn login_title() -> &'static str {
    "欢迎王者归来"
}

pub const fn login_button_text() -> &'static str {
    "登录"
}

pub fn status_copy(tone: StatusCopyTone, index: usize) -> &'static str {
    let pool = match tone {
        StatusCopyTone::Launching => &["你的电子牛马已就位。", "电子牛马正在赶路。"][..],
        StatusCopyTone::RedeemSuccess => {
            &["电子牛马获得了一次投喂。", "这次投喂到账，电子牛马干劲很足。"][..]
        }
        StatusCopyTone::BillingDue => {
            &["给你的电子牛马喂点草料。", "电子牛马今天快没力气了。"][..]
        }
        StatusCopyTone::Retry => {
            &["电子牛马刚刚打了个盹，正在重新探路。", "电子牛马没找到路，正在重新探路。"][..]
        }
        StatusCopyTone::Cleanup => {
            &["电子牛马已安全收工，原环境保持干净。", "今天这趟活已经收尾，环境也替你收干净了。"][..]
        }
    };
    pool[index % pool.len()]
}

#[cfg(test)]
mod tests {
    use super::{login_title, product_name, status_copy, StatusCopyTone};

    #[test]
    fn product_copy_matches_approved_brand() {
        assert_eq!(product_name(), "一键开整");
        assert_eq!(login_title(), "欢迎王者归来");
        assert_eq!(
            status_copy(StatusCopyTone::BillingDue, 0),
            "给你的电子牛马喂点草料。"
        );
        assert_ne!(product_name(), "Sub2API Desktop");
    }
}
