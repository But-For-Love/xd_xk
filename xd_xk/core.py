"""核心业务逻辑：登录、验证码、选课、退课."""

from __future__ import annotations

import base64
import json
import logging
import tempfile
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

import ddddocr
import requests
from PIL import Image as PILImage

from xd_xk.encrypt import AES_encrypt

logger = logging.getLogger(__name__)

# ── 类型别名 ──────────────────────────────────────────────────────
LogFunc = Callable[[str], None] | None
JsonDict = dict[str, Any]


# ═══════════════════════════════════════════════════════════════════
#  工具函数
# ═══════════════════════════════════════════════════════════════════


def _log(msg: str, log_func: LogFunc = None) -> None:
    """统一日志输出：有回调用回调，否则用 logging."""
    if log_func:
        log_func(msg)
    else:
        logger.info(msg)


def _show_captcha_manual(image_bytes: bytes) -> str:
    """用系统图片查看器显示验证码，等待用户手动输入."""
    with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
        f.write(image_bytes)
        tmp_path = f.name
    try:
        img = PILImage.open(tmp_path)
        img.show()
    finally:
        # 延迟清理，等用户关闭查看器
        pass
    code = input("请输入验证码: ")
    Path(tmp_path).unlink(missing_ok=True)
    return code


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


def get_captcha(conf: JsonDict, log_func: LogFunc = None) -> tuple[str, str]:
    """获取验证码，返回 (code, uuid).

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

    if conf.get("debug") == "1":
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

    if conf.get("ocr_captcha") == "1":
        try:
            code = ocr_captcha(img_bytes)
        except Exception as e:
            raise RuntimeError(f"验证码 OCR 识别失败：{type(e).__name__}: {e}")
        _log(f"验证码识别结果：{code}", log_func)
    else:
        code = _show_captcha_manual(img_bytes)

    return code, p["data"]["uuid"]


# ═══════════════════════════════════════════════════════════════════
#  登录
# ═══════════════════════════════════════════════════════════════════


def login(conf: JsonDict, log_func: LogFunc = None) -> tuple[JsonDict, dict[str, str]]:
    """登录选课系统，返回 (json_data, cookie_dict).

    Raises:
        RuntimeError: 登录失败时抛出.
    """
    url = f"{BASE_URL}/auth/login"

    form = dict(conf["data"])
    if not form.get("loginname") or not form.get("password"):
        form["loginname"] = input("学号：")
        form["password"] = input("密码：")
    form["password"] = AES_encrypt(form["password"])
    form["captcha"], form["uuid"] = get_captcha(conf, log_func=log_func)

    _log(f"正在登录… 学号：{form['loginname']}", log_func)

    try:
        result = requests.post(url, _api_headers(), params=form, timeout=15)
    except requests.exceptions.Timeout:
        raise RuntimeError("登录接口超时（15s），请检查网络连接")
    except requests.exceptions.ConnectionError:
        raise RuntimeError("无法连接到选课服务器，请检查网络或 VPN")
    except requests.exceptions.RequestException as e:
        raise RuntimeError(f"登录请求失败：{type(e).__name__}: {e}")

    if conf.get("debug") == "1":
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


def show_msg(
    data: JsonDict,
    log_func: LogFunc = None,
    batch_name: str = "2025级",
) -> str:
    """显示学生信息并匹配选课批次，返回 batch_code.

    Raises:
        RuntimeError: 数据异常或批次不匹配时抛出.
    """
    batch_code = ""
    try:
        student = data["data"]["student"]
        _log(f"姓名：{student['XM']}", log_func)
        _log(f"专业：{student['ZYMC']}", log_func)
        _log(f"班级：{student['schoolClass']}", log_func)
        lst = student["electiveBatchList"]
        if not lst:
            raise RuntimeError("electiveBatchList 为空，没有可用的选课批次")

        matched_but_not_open: list[str] = []
        name_matched_open: list[str] = []
        name_not_found = True

        for batch in lst:
            _log(
                f"  选课批次：{batch['name']}　可选：{'是' if batch['canSelect'] == '1' else '否'}",
                log_func,
            )
            if batch_name in batch["name"]:
                name_not_found = False
                if batch["canSelect"] == "1":
                    batch_code = batch["code"]
                    name_matched_open.append(batch["name"])
                else:
                    matched_but_not_open.append(batch["name"])

        if not batch_code:
            if name_not_found:
                raise RuntimeError(
                    f"未找到包含「{batch_name}」的选课批次\n全部批次：{[i['name'] for i in lst]}"
                )
            if matched_but_not_open:
                raise RuntimeError(f"本轮选课暂未开始：{matched_but_not_open}\n请等待开放后再试")
            avail = [i["name"] for i in lst if i["canSelect"] == "1"]
            raise RuntimeError(f"匹配到批次但未获取到 code\n可选批次：{avail if avail else '无'}")
    except (TypeError, KeyError) as e:
        detail = json.dumps(data, ensure_ascii=False)[:500]
        raise RuntimeError(f"解析学生信息失败：{type(e).__name__}: {e}\n完整响应：{detail}")
    return batch_code


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

    if conf.get("debug") == "1":
        Path("classlist.json").write_bytes(resp.content)

    return resp.json()


# ═══════════════════════════════════════════════════════════════════
#  选课 / 退课
# ═══════════════════════════════════════════════════════════════════

_STOP_MSGS = (
    "该课程已在选课结果中",
    "所选课程与已选课程冲突",
    "所选课程人数已满",
    "操作成功",
    "选课门数或学分超过",
)


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

    if always == 1:
        k = 1
        msg = ""
        while not any(s in msg for s in _STOP_MSGS):
            if stop_event and stop_event.is_set():
                _log("用户停止操作", log_func)
                return
            r = requests.post(url, params=form, headers=headers, cookies=cookie)
            msg = r.json()["msg"]
            _log(
                f"{class_dict['KCH']} {class_dict['KCM']}\t选课\t{msg}{'-' * (k % 10)}",
                log_func,
            )
            k += 1
            time.sleep(1)
    else:
        r = requests.post(url, params=form, headers=headers, cookies=cookie)
        msg = r.json()["msg"]
        _log(f"{class_dict['KCH']} {class_dict['KCM']}\t选课\t{msg}", log_func)


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

    if always == 1:
        msg = ""
        while msg not in ("所选课程与已选课程冲突", "操作成功"):
            if stop_event and stop_event.is_set():
                _log("用户停止操作", log_func)
                return
            r = requests.post(url, params=form, headers=headers, cookies=cookie)
            msg = r.json()["msg"]
            _log(
                f"{class_dict['KCH']} {class_dict['KCM']}\t"
                f"{class_dict.get('SKJS', '')}\t退课\t{msg}",
                log_func,
            )
    else:
        r = requests.post(url, params=form, headers=headers, cookies=cookie)
        msg = r.json()["msg"]
        _log(
            f"{class_dict['KCH']} {class_dict['KCM']}\t{class_dict.get('SKJS', '')}\t退课\t{msg}",
            log_func,
        )
