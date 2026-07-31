"""核心业务逻辑：登录、验证码、选课、退课."""

from __future__ import annotations

import base64
import json
import logging
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import ddddocr
import requests

from xd_xk.encrypt import AES_encrypt

logger = logging.getLogger(__name__)

# ── 类型别名 ──────────────────────────────────────────────────────
LogFunc = Callable[[str], None] | None
JsonDict = dict[str, Any]


# ═══════════════════════════════════════════════════════════════════
#  配置辅助
# ═══════════════════════════════════════════════════════════════════


def _conf_bool(conf: JsonDict, key: str) -> bool:
    """读取字符串布尔型配置为 Python bool."""
    return conf.get(key) == "1"


# ═══════════════════════════════════════════════════════════════════
#  会话对象（外观模式 — 封装登录状态）
# ═══════════════════════════════════════════════════════════════════


@dataclass
class CourseSession:
    """封装一个已认证的选课会话.

    将散落的 token / cookie / batch_code 打包为单一对象，
    避免到处传递裸 dict.
    """

    token: str
    cookie: dict[str, str]
    batch_code: str
    data: JsonDict  # 原始登录响应，保留用于兼容

    @classmethod
    def create(
        cls,
        conf: JsonDict,
        log_func: LogFunc = None,
        captcha_cb: Callable[[bytes], str] | None = None,
    ) -> CourseSession:
        """工厂方法：登录 → 展示信息 → 匹配批次 → 返回会话.

        这是 GUI/CLI 中最常见的启动流程.
        """
        jd, ck = login(conf, log_func=log_func, captcha_cb=captcha_cb)
        batch_name = conf.get("batch_name", "")
        ba = show_msg(jd, log_func=log_func, batch_name=batch_name)
        return cls(
            token=jd["data"]["token"],
            cookie=ck,
            batch_code=ba,
            data=jd,
        )


# ═══════════════════════════════════════════════════════════════════
#  工具函数
# ═══════════════════════════════════════════════════════════════════


def _log(msg: str, log_func: LogFunc = None) -> None:
    """统一日志输出：有回调用回调，否则用 logging."""
    if log_func:
        log_func(msg)
    else:
        logger.info(msg)


# 注意：人工验证码输入已从 core 层移除。
# 调用方如需手动输入验证码，应通过 captcha_cb 回调自行实现。
# CLI 入口在 cli.py 中提供了参考实现.


# ═══════════════════════════════════════════════════════════════════
#  HTTP 公共头
# ═══════════════════════════════════════════════════════════════════

_USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44"
)

BASE_URL = "https://xk.xidian.edu.cn/xsxk"


def _api_headers(
    token: str | None = None,
    batch_id: str | None = None,
) -> dict[str, str]:
    """构建请求头."""
    h: dict[str, str] = {
        "Connection": "keep-alive",
        "User-Agent": _USER_AGENT,
    }
    if token:
        h["Authorization"] = token
    if batch_id:
        h["batchId"] = batch_id
    return h


# ═══════════════════════════════════════════════════════════════════
#  验证码
# ═══════════════════════════════════════════════════════════════════


def ocr_captcha(img: bytes) -> str:
    """OCR 识别验证码图片."""
    ocr = ddddocr.DdddOcr()
    return ocr.classification(img)


def get_captcha(
    conf: JsonDict,
    log_func: LogFunc = None,
    captcha_cb: Callable[[bytes], str] | None = None,
) -> tuple[str, str]:
    """获取验证码，返回 (code, uuid).

    Args:
        captcha_cb: 人工验证码回调，接收图片 bytes，返回用户输入的验证码。
                    仅当 ocr_captcha=0 时调用。不提供则 OCR 关闭时报错。

    Raises:
        RuntimeError: 网络异常、接口异常或识别失败时抛出.
    """
    url = f"{BASE_URL}/auth/captcha"
    try:
        result = requests.post(url, timeout=10)
    except requests.exceptions.Timeout:
        raise RuntimeError("验证码接口超时（10s），请检查网络连接")
    except requests.exceptions.ConnectionError:
        raise RuntimeError("无法连接到选课服务器 xk.xidian.edu.cn，请检查网络或 VPN")
    except requests.exceptions.RequestException as e:
        raise RuntimeError(f"验证码请求失败：{type(e).__name__}: {e}")

    if result.status_code != 200:
        raise RuntimeError(f"验证码接口返回 HTTP {result.status_code}，响应：{result.text[:200]}")

    try:
        p = result.json()
    except Exception:
        raise RuntimeError(f"验证码接口返回非 JSON 数据：{result.text[:200]}")

    if _conf_bool(conf, "debug"):
        Path("captcha_pac.json").write_bytes(result.content)

    _log(f"验证码接口：{p.get('msg', '无 msg')}", log_func)

    if "data" not in p or "captcha" not in p.get("data", {}):
        raise RuntimeError(
            f"验证码接口返回异常，缺少 data.captcha 字段。"
            f"完整响应：{json.dumps(p, ensure_ascii=False)[:300]}"
        )

    pic = p["data"]["captcha"].replace("data:image/png;base64,", "")
    try:
        img_bytes = base64.b64decode(pic)
    except Exception as e:
        raise RuntimeError(f"验证码图片 Base64 解码失败：{e}")

    if _conf_bool(conf, "ocr_captcha"):
        try:
            code = ocr_captcha(img_bytes)
        except Exception as e:
            raise RuntimeError(f"验证码 OCR 识别失败：{type(e).__name__}: {e}")
        _log(f"验证码识别结果：{code}", log_func)
    elif captcha_cb:
        code = captcha_cb(img_bytes)
    else:
        raise RuntimeError(
            "验证码需要人工输入（ocr_captcha=0），但未提供 captcha_cb 回调。"
            "请启用自动验证码或提供人工输入回调。"
        )

    return code, p["data"]["uuid"]


# ═══════════════════════════════════════════════════════════════════
#  登录
# ═══════════════════════════════════════════════════════════════════


def login(
    conf: JsonDict,
    log_func: LogFunc = None,
    captcha_cb: Callable[[bytes], str] | None = None,
) -> tuple[JsonDict, dict[str, str]]:
    """登录选课系统，返回 (json_data, cookie_dict).

    Args:
        captcha_cb: 人工验证码回调，传给 get_captcha。

    Raises:
        RuntimeError: 登录失败时抛出.
    """
    url = f"{BASE_URL}/auth/login"

    form = dict(conf["data"])
    if not form.get("loginname") or not form.get("password"):
        raise RuntimeError(
            "配置中缺少学号或密码，请在 conf.json 的 data.loginname / data.password 中填写"
        )
    form["password"] = AES_encrypt(form["password"])
    form["captcha"], form["uuid"] = get_captcha(conf, log_func=log_func, captcha_cb=captcha_cb)

    _log(f"正在登录… 学号：{form['loginname']}", log_func)

    try:
        result = requests.post(url, _api_headers(), params=form, timeout=15)
    except requests.exceptions.Timeout:
        raise RuntimeError("登录接口超时（15s），请检查网络连接")
    except requests.exceptions.ConnectionError:
        raise RuntimeError("无法连接到选课服务器，请检查网络或 VPN")
    except requests.exceptions.RequestException as e:
        raise RuntimeError(f"登录请求失败：{type(e).__name__}: {e}")

    if _conf_bool(conf, "debug"):
        Path("login_pac.json").write_bytes(result.content)

    if result.status_code != 200:
        raise RuntimeError(f"登录接口返回 HTTP {result.status_code}，响应：{result.text[:300]}")

    try:
        resp = result.json()
    except Exception:
        raise RuntimeError(f"登录接口返回非 JSON：{result.text[:300]}")

    code = resp.get("code")
    msg = resp.get("msg", "")
    _log(f"登录响应：code={code}, msg={msg}", log_func)

    if code != 200:
        detail = json.dumps(resp, ensure_ascii=False)[:500]
        raise RuntimeError(f"登录失败（code={code}）：{msg}\n完整响应：{detail}")

    if "data" not in resp or "token" not in resp.get("data", {}):
        raise RuntimeError(
            f"登录响应缺少 token，完整响应：{json.dumps(resp, ensure_ascii=False)[:500]}"
        )

    _log("[OK] 登录成功", log_func)
    return resp, requests.utils.dict_from_cookiejar(result.cookies)


# ═══════════════════════════════════════════════════════════════════
#  学生信息 & 批次
# ═══════════════════════════════════════════════════════════════════


def _log_student_info(student: JsonDict, log_func: LogFunc = None) -> None:
    """展示学生基本信息."""
    _log(f"姓名：{student['XM']}", log_func)
    _log(f"专业：{student['ZYMC']}", log_func)
    _log(f"班级：{student['schoolClass']}", log_func)


def _list_batches(batches: list[JsonDict], log_func: LogFunc = None) -> None:
    """列出所有可选批次."""
    for b in batches:
        can = "是" if b["canSelect"] == "1" else "否"
        _log(f"  选课批次：{b['name']}　可选：{can}", log_func)


def _match_batch(
    batches: list[JsonDict],
    batch_name: str,
) -> str:
    """在批次列表中按名称关键字匹配，返回 batch_code.

    Raises:
        RuntimeError: 未匹配到或批次未开放.
    """
    matched_open: list[str] = []
    matched_closed: list[str] = []
    batch_code = ""

    for b in batches:
        if batch_name in b["name"]:
            if b["canSelect"] == "1":
                batch_code = b["code"]
                matched_open.append(b["name"])
            else:
                matched_closed.append(b["name"])

    if batch_code:
        return batch_code

    # ── 以下为异常路径 ──
    if not matched_open and not matched_closed:
        names = [i["name"] for i in batches]
        raise RuntimeError(f"未找到包含「{batch_name}」的选课批次\n全部批次：{names}")
    if matched_closed:
        raise RuntimeError(f"本轮选课暂未开始：{matched_closed}\n请等待开放后再试")
    avail = [i["name"] for i in batches if i["canSelect"] == "1"]
    raise RuntimeError(f"匹配到批次但未获取到 code\n可选批次：{avail if avail else '无'}")


def get_batch_list(data: JsonDict) -> list[JsonDict]:
    """从登录数据中提取可选批次列表（需先登录）。

    Returns:
        批次列表，每项含 name / code / canSelect 等字段。
    """
    return data["data"]["student"]["electiveBatchList"]


def match_batch_name(batches: list[JsonDict], keyword: str) -> str:
    """在批次列表中按名称关键字匹配，返回 batch_code。

    等价于 _match_batch，但对调用方隐藏内部实现细节。
    """
    return _match_batch(batches, keyword)


def show_msg(
    data: JsonDict,
    log_func: LogFunc = None,
    batch_name: str = "2025级",
) -> str:
    """显示学生信息并匹配选课批次，返回 batch_code.

    Raises:
        RuntimeError: 数据异常或批次不匹配时抛出.
    """
    try:
        student = data["data"]["student"]
        lst = student["electiveBatchList"]
        if not lst:
            raise RuntimeError("electiveBatchList 为空，没有可用的选课批次")

        _log_student_info(student, log_func)
        _list_batches(lst, log_func)
        return _match_batch(lst, batch_name)
    except (TypeError, KeyError) as e:
        detail = json.dumps(data, ensure_ascii=False)[:500]
        raise RuntimeError(f"解析学生信息失败：{type(e).__name__}: {e}\n完整响应：{detail}")


def choose_batch(data: JsonDict, batch_id: str) -> JsonDict:
    """切换选课批次."""
    url = f"{BASE_URL}/elective/user"
    headers = _api_headers(token=data["data"]["token"])
    form = {"batchId": batch_id}
    resp = requests.post(url, params=form, headers=headers)
    return resp.json()


# ═══════════════════════════════════════════════════════════════════
#  课程列表
# ═══════════════════════════════════════════════════════════════════


def get_class(
    data: JsonDict,
    conf: JsonDict,
    batch: str,
    category: int = 0,
) -> JsonDict:
    """获取课程列表.

    Args:
        data: 登录返回的 json
        conf: 配置
        batch: 批次 code
        category: 0=必修(FANKC), 1=选修(XGKC)
    """
    url = f"{BASE_URL}/elective/clazz/list"
    headers = _api_headers(token=data["data"]["token"], batch_id=batch)
    headers["Content-Type"] = "application/json;charset=UTF-8"

    cat = ["FANKC", "XGKC"]
    form = {
        "teachingClassType": cat[category],
        "pageNumber": 1,
        "pageSize": 300,
        "orderBy": "",
        "campus": "S",
    }

    resp = requests.post(url, json=form, headers=headers)

    if _conf_bool(conf, "debug"):
        Path("classlist.json").write_bytes(resp.content)

    return resp.json()


def fetch_courses(
    data: JsonDict,
    conf: JsonDict,
    batch: str,
    categories: set[int],
) -> dict[int, list[JsonDict]]:
    """批量获取指定类别的课程列表.

    Args:
        data: 登录返回的 json
        conf: 配置
        batch: 批次 code
        categories: 要获取的类别集合，0=必修 1=选修

    Returns:
        {category: [course_rows], ...}
    """
    rows_by_cat: dict[int, list[JsonDict]] = {}
    for cat in categories:
        resp = get_class(data, conf, batch=batch, category=cat)
        rows_by_cat[cat] = resp.get("data", {}).get("rows", [])
    return rows_by_cat


# ═══════════════════════════════════════════════════════════════════
#  选课 / 退课（模板方法模式 — 提取公共轮询逻辑）
# ═══════════════════════════════════════════════════════════════════

_STOP_MSGS = (
    "该课程已在选课结果中",
    "所选课程与已选课程冲突",
    "所选课程人数已满",
    "操作成功",
    "选课门数或学分超过",
)


def _poll_operation(
    url: str,
    params: dict[str, str],
    headers: dict[str, str],
    cookies: dict[str, str],
    label: str,
    stop_condition: Callable[[str], bool],
    log_func: LogFunc = None,
    stop_event: Any = None,
) -> None:
    """轮询发送选课/退课请求，直到满足停止条件或被中断.

    模板方法：add/dele 只需提供各自的 URL、form、日志前缀和停止条件.
    """
    k = 1
    msg = ""
    while not stop_condition(msg):
        if stop_event and stop_event.is_set():
            _log("用户停止操作", log_func)
            return
        r = requests.post(url, params=params, headers=headers, cookies=cookies)
        msg = r.json()["msg"]
        _log(f"{label}\t{msg}{'-' * (k % 10)}", log_func)
        k += 1
        time.sleep(1)


def add(
    data: JsonDict,
    class_dict: JsonDict,
    cookie: dict[str, str],
    batch: str,
    always: int = 1,
    category: int = 0,
    log_func: LogFunc = None,
    stop_event: Any = None,
) -> None:
    """选课.

    Args:
        always: 1=持续重试直到成功或匹配到终止消息, 0=单次请求
        stop_event: threading.Event，设置后停止重试
    """
    url = f"{BASE_URL}/elective/clazz/add"
    headers = _api_headers(token=data["data"]["token"], batch_id=batch)
    clazz_type = "FANKC" if category == 0 else "XGKC"
    form = {
        "clazzType": clazz_type,
        "clazzId": class_dict["JXBID"],
        "secretVal": class_dict["secretVal"],
        "chooseVolunteer": "1",
    }
    cookie["Authorization"] = data["data"]["token"]
    label = f"{class_dict['KCH']} {class_dict['KCM']}\t选课"

    if always == 1:
        _poll_operation(
            url,
            form,
            headers,
            cookie,
            label,
            stop_condition=lambda msg: any(s in msg for s in _STOP_MSGS),
            log_func=log_func,
            stop_event=stop_event,
        )
    else:
        r = requests.post(url, params=form, headers=headers, cookies=cookie)
        _log(f"{label}\t{r.json()['msg']}", log_func)


def dele(
    data: JsonDict,
    class_dict: JsonDict,
    cookie: dict[str, str],
    batch: str,
    always: int = 1,
    category: int = 0,
    log_func: LogFunc = None,
    stop_event: Any = None,
) -> None:
    """退课.

    Args:
        always: 1=持续重试, 0=单次请求
        stop_event: threading.Event，设置后停止重试
    """
    url = f"{BASE_URL}/elective/clazz/del"
    headers = _api_headers(token=data["data"]["token"], batch_id=batch)
    clazz_type = "TJKC" if category == 0 else "XGKC"

    form: dict[str, str] = {
        "clazzType": clazz_type,
        "clazzId": class_dict["JXBID"],
        "secretVal": class_dict["secretVal"],
    }
    if category == 1:
        form["chooseVolunteer"] = "1"
    cookie["Authorization"] = data["data"]["token"]
    label = f"{class_dict['KCH']} {class_dict['KCM']}\t{class_dict.get('SKJS', '')}\t退课"

    if always == 1:
        _poll_operation(
            url,
            form,
            headers,
            cookie,
            label,
            stop_condition=lambda msg: msg in ("所选课程与已选课程冲突", "操作成功"),
            log_func=log_func,
            stop_event=stop_event,
        )
    else:
        r = requests.post(url, params=form, headers=headers, cookies=cookie)
        _log(f"{label}\t{r.json()['msg']}", log_func)
