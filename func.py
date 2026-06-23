import requests
import json
import base64
import ddddocr
import os
from requests import utils
from encrypt import AES_encrypt
import time
import threading

# matplotlib 仅在手动输入验证码时按需导入
plt = None


def _log(msg, log_func=None):
    """统一日志输出：有回调用回调，否则 print"""
    if log_func:
        log_func(msg)
    else:
        print(msg)


def ocr_captcha(img):
    """
    :return: 识别到的验证码
    """
    ocr = ddddocr.DdddOcr()
    code_bytes = img
    return ocr.classification(code_bytes)


def get_captcha(conf, log_func=None):
    """
    :param conf: conf.json
    :param log_func: 日志回调函数
    :return: code & uuid
    :raises RuntimeError: 验证码获取失败时
    """
    url = "https://xk.xidian.edu.cn/xsxk/auth/captcha"
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

    if conf['debug'] == '1':
        with open("captcha_pac.json", "wb") as f:
            f.write(result.content)

    _log(f"验证码接口：{p.get('msg', '无 msg')}", log_func)

    if 'data' not in p or 'captcha' not in p.get('data', {}):
        raise RuntimeError(f"验证码接口返回异常，缺少 data.captcha 字段。完整响应：{json.dumps(p, ensure_ascii=False)[:300]}")

    pic = p['data']['captcha'].replace("data:image/png;base64,", "")
    try:
        b = base64.b64decode(pic)
    except Exception as e:
        raise RuntimeError(f"验证码图片 Base64 解码失败：{e}")

    if conf["ocr_captcha"] == "1":      # 默认，自动识别验证码
        try:
            code = ocr_captcha(b)
        except Exception as e:
            raise RuntimeError(f"验证码 OCR 识别失败：{type(e).__name__}: {e}")
        _log(f"验证码识别结果：{code}", log_func)
    else:                               # 手动输入
        global plt
        if plt is None:
            import matplotlib.pyplot as _plt
            plt = _plt
        img = plt.imread(p['data']['captcha'])
        plt.imshow(img)
        plt.show()
        code = input("请输入验证码:")

    return code, p['data']['uuid']


def login(conf, log_func=None):
    """
    :param conf: conf.json
    :param log_func: 日志回调函数
    :return: (json_data, cookie_dict)
    :raises RuntimeError: 登录失败时
    """
    url = "https://xk.xidian.edu.cn/xsxk/auth/login"

    header = {
        "Connection": "keep-alive",
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
    }

    form = conf["data"]
    if conf["data"]["loginname"] == "" or conf["data"]["password"] == "":
        form["loginname"] = input("学号：")
        form["password"] = input("密码：")
    form["password"] = AES_encrypt(form["password"])
    form["captcha"], form["uuid"] = get_captcha(conf, log_func=log_func)

    _log(f"正在登录… 学号：{form['loginname']}", log_func)

    try:
        result = requests.post(url, header, params=form, timeout=15)
    except requests.exceptions.Timeout:
        raise RuntimeError("登录接口超时（15s），请检查网络连接")
    except requests.exceptions.ConnectionError:
        raise RuntimeError("无法连接到选课服务器，请检查网络或 VPN")
    except requests.exceptions.RequestException as e:
        raise RuntimeError(f"登录请求失败：{type(e).__name__}: {e}")

    if conf['debug'] == '1':
        with open("login_pac.json", "wb") as f:
            f.write(result.content)

    if result.status_code != 200:
        raise RuntimeError(f"登录接口返回 HTTP {result.status_code}，响应：{result.text[:300]}")

    try:
        resp = result.json()
    except Exception:
        raise RuntimeError(f"登录接口返回非 JSON：{result.text[:300]}")

    code = resp.get("code")
    msg  = resp.get("msg", "")
    _log(f"登录响应：code={code}, msg={msg}", log_func)

    if code != 200:
        detail = json.dumps(resp, ensure_ascii=False)[:500]
        raise RuntimeError(f"登录失败（code={code}）：{msg}\n完整响应：{detail}")

    if "data" not in resp or "token" not in resp.get("data", {}):
        raise RuntimeError(f"登录响应缺少 token，完整响应：{json.dumps(resp, ensure_ascii=False)[:500]}")

    _log("[OK] 登录成功", log_func)
    return resp, requests.utils.dict_from_cookiejar(result.cookies)


def show_msg(json, log_func=None, batch_name="2025级"):
    """
    :param json:        登录成功后返回的json
    :param log_func:    日志回调函数
    :param batch_name:  选课批次名称关键字，用于匹配
    :return:            batch_code
    :raises RuntimeError: 数据异常时
    """
    batch_code = ''
    try:
        student = json["data"]["student"]
        _log(f"姓名：{student['XM']}", log_func)
        _log(f"专业：{student['ZYMC']}", log_func)
        _log(f"班级：{student['schoolClass']}", log_func)
        lst = student["electiveBatchList"]
        if not lst:
            raise RuntimeError("electiveBatchList 为空，没有可用的选课批次")

        matched_but_not_open = []   # 名称匹配但尚未开放
        name_matched_open = []      # 名称匹配且已开放
        name_not_found = True       # 名称是否完全没匹配到

        for i in lst:
            _log(f"  选课批次：{i['name']}　可选：{'是' if i['canSelect'] == '1' else '否'}", log_func)
            if batch_name in i["name"]:
                name_not_found = False
                if i["canSelect"] == "1":
                    batch_code = i["code"]
                    name_matched_open.append(i["name"])
                else:
                    matched_but_not_open.append(i["name"])

        if not batch_code:
            if name_not_found:
                # 批次名根本不存在
                raise RuntimeError(
                    f"未找到包含「{batch_name}」的选课批次\n"
                    f"全部批次：{[i['name'] for i in lst]}"
                )
            elif matched_but_not_open:
                # 批次名存在但还没开放
                raise RuntimeError(
                    f"本轮选课暂未开始：{matched_but_not_open}\n"
                    f"请等待开放后再试"
                )
            else:
                # 有名称匹配且开放的，但 code 为空（异常情况）
                avail = [i['name'] for i in lst if i['canSelect'] == '1']
                raise RuntimeError(
                    f"匹配到批次但未获取到 code\n"
                    f"可选批次：{avail if avail else '无'}"
                )
    except (TypeError, KeyError) as e:
        detail = json.dumps(json, ensure_ascii=False)[:500]
        raise RuntimeError(
            f"解析学生信息失败：{type(e).__name__}: {e}\n"
            f"完整响应：{detail}"
        )
    return batch_code


def choose_Batch(j, batch):
    """
    :param batch:
    :param j: login.json
    :return:
    不需要
    """
    url = 'https://xk.xidian.edu.cn/xsxk/elective/user'

    header = {
        "Connection": "keep-alive",
        "Authorization": j["data"]["token"],
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
    }
    form = {
        "batchId": batch
    }

    h = requests.post(url, params=form, headers=header)
    with open("batch_pac.json", "wb") as f:
        f.write(h.content)  # 字节形式写入，保存为json文件
    # print(h.text)
    return h.json()


def get_class(j, conf, batch, category=0):
    url = "https://xk.xidian.edu.cn/xsxk/elective/clazz/list"
    header = {
        "Connection": "keep-alive",
        "Content-Type": "application/json;charset=UTF-8",
        "batchId": batch,
        "Authorization": j["data"]["token"],
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
    }
    cat = ["FANKC", "XGKC"]
    form = {
            "teachingClassType": cat[category],
            "pageNumber": 1,
            "pageSize": 300,
            "orderBy": "",
            "campus": "S"
    }
    cookie = {"Authorization": j["data"]["token"]}
    a = requests.post(url, json=form, headers=header)

    if conf['debug'] == '1':
        with open("classlist.json", "wb") as f:
            f.write(a.content)  # 字节形式写入，保存为json文件
    # print(a.text)
    return a.json()


def add(j, class_dict, cookie, batch, always=1, category=0, log_func=None, stop_event=None):
    """
    :param stop_event: threading.Event，用于停止连续重试
    """
    header = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
        "batchId": batch,
        "Authorization": j["data"]["token"]
    }

    url = 'https://xk.xidian.edu.cn/xsxk/elective/clazz/add'

    if category == 0:        # 必修
        form = {
            "clazzType": "FANKC",
            "clazzId": class_dict["JXBID"],
            "secretVal": class_dict["secretVal"],
            "chooseVolunteer": "1"
        }
    elif category == 1:      # 选修
        form = {
            "clazzType": "XGKC",
            "clazzId": class_dict["JXBID"],
            "secretVal": class_dict["secretVal"],
            "chooseVolunteer": "1"
        }

    cookie["Authorization"] = j["data"]["token"]
    # 不需要重试的情况：已选上、冲突、已满、门数/学分超限、选课成功
    _stop_msgs = ['该课程已在选课结果中', '所选课程与已选课程冲突',
                  '所选课程人数已满', '操作成功', '选课门数或学分超过']
    k = 1
    if always == 1:
        msg = ''
        while not any(s in msg for s in _stop_msgs):
            if stop_event and stop_event.is_set():
                _log("用户停止操作", log_func)
                return
            r = requests.post(url, params=form, headers=header, cookies=cookie)
            msg = r.json()["msg"]
            _log(f"{class_dict['KCH']} {class_dict['KCM']}\t选课\t{msg}{'-' * k}", log_func)
            k += 1
            k = k % 10
            time.sleep(1)
    else:
        r = requests.post(url, params=form, headers=header, cookies=cookie)
        msg = r.json()["msg"]
        _log(f"{class_dict['KCH']} {class_dict['KCM']}\t选课\t{msg}", log_func)


def dele(j, class_dict, cookie, batch, always=1, category=0, log_func=None, stop_event=None):
    """
    :param stop_event: threading.Event，用于停止连续重试
    """
    header = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
        "batchId": batch,
        "Authorization": j["data"]["token"]
    }

    url = 'https://xk.xidian.edu.cn/xsxk/elective/clazz/del'

    if category == 0:        # 必修
        form = {
            "clazzType": "TJKC",
            "clazzId": class_dict["JXBID"],
            "secretVal": class_dict["secretVal"]
        }
    elif category == 1:      # 选修
        form = {
            "clazzType": "XGKC",
            "clazzId": class_dict["JXBID"],
            "secretVal": class_dict["secretVal"],
            "chooseVolunteer": "1"
        }

    cookie["Authorization"] = j["data"]["token"]

    if always == 1:
        msg = ''
        while msg not in ['所选课程与已选课程冲突', '操作成功']:
            if stop_event and stop_event.is_set():
                _log("用户停止操作", log_func)
                return
            r = requests.post(url, params=form, headers=header, cookies=cookie)
            msg = r.json()["msg"]
            _log(f"{class_dict['KCH']} {class_dict['KCM']}\t{class_dict.get('SKJS', '')}\t退课\t{msg}", log_func)
    else:
        r = requests.post(url, params=form, headers=header, cookies=cookie)
        msg = r.json()["msg"]
        _log(f"{class_dict['KCH']} {class_dict['KCM']}\t{class_dict.get('SKJS', '')}\t退课\t{msg}", log_func)


if __name__ == '__main__':
    pass
    # 去运行 xk_main.py

