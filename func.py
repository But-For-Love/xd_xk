import requests
import json
import base64
import matplotlib.pyplot as plt
import ddddocr
import os
from requests import utils
from encrypt import AES_encrypt
import time


def ocr_captcha(img):
    """
    :return: 识别到的验证码
    """
    ocr = ddddocr.DdddOcr()
    code_bytes = img
    return ocr.classification(code_bytes)


def get_captcha(conf):
    """
    :param conf: conf.json
    :return: code & uuid
    """
    url = "https://xk.xidian.edu.cn/xsxk/auth/captcha"
    result = requests.post(url)
    p = result.json()
    if conf['debug'] == '1':
        with open("captcha_pac.json", "wb") as f:
            f.write(result.content)      # 字节形式写入，保存为json文件

    print(p['msg'])   # 打印状态

    pic = p['data']['captcha'].replace("data:image/png;base64,", "")
    b = base64.b64decode(pic)       # 用于ddddocr识别

    if conf["ocr_captcha"] == "1":      # 默认，自动识别验证码
        code = ocr_captcha(b)
        print("验证码为:", code)
    else:                               # 手动输入
        img = plt.imread(p['data']['captcha'])
        plt.imshow(img)
        plt.show()                      # 显示验证码
        code = input("请输入验证码:")

    return code, p['data']['uuid']


def login(conf):
    """
    :param conf: conf.json
    :return: 登录后返回的json
    """
    url = "https://xk.xidian.edu.cn/xsxk/auth/login"

    header = {
        "Connection": "keep-alive",
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
    }

    form = conf["data"]
    if conf["data"]["loginname"] == "" or conf["data"]["password"] == "":   # 如果缺少 用户名 和 密码
        form["loginname"] = input("学号：")
        form["password"] = input("密码：")
    form["password"] = AES_encrypt(form["password"])
    form["captcha"], form["uuid"] = get_captcha(conf)   # 构造表单

    result = requests.post(url, header, params=form)
    if conf['debug'] == '1':
        with open("login_pac.json", "wb") as f:
            f.write(result.content)      # 字节形式写入，保存为json文件

    return result.json(), requests.utils.dict_from_cookiejar(result.cookies)
    # return result.json()


def show_msg(json):
    """
    :param json:    登录成功后返回的json
    :return:        NONE
    """
    try:
        print("姓名：", json["data"]["student"]["XM"])
        print("专业：", json["data"]["student"]["ZYMC"])
        print("班级：", json["data"]["student"]["schoolClass"])
        lst = json["data"]["student"]["electiveBatchList"]
        for i in lst:
            print("选课批次：", i["name"], "\t是否可选：", i["canSelect"])
            if i["canSelect"] == "1" and "2020" in i["name"]:
                batch_code = i["code"]
    except TypeError:
        print(json["msg"])
        batch_code = ''
    # print(batch_code)
    # print(json["data"]["token"])
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
    cat = ["TJKC", "XGKC"]
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


def add(j, class_dict, cookie, batch, always=1, category=0):

    header = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
        "batchId": batch,
        "Authorization": j["data"]["token"]
    }

    # url1 = 'https://xk.xidian.edu.cn/xsxk/volunteer/list/choose'
    # form1 = {
    #     "clazzType": "TJKC",
    #     "clazzId": clazzId
    # }
    # r1 = requests.post(url1, params=form1, headers=header)
    # print(r1.text)

    # time.sleep(1)

    url = 'https://xk.xidian.edu.cn/xsxk/elective/clazz/add'

    if category == 0:        # 必修
        form = {
            "clazzType": "TJKC",
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

    if always == 1:
        msg = ''
        while msg not in ['该课程已在选课结果中', '所选课程与已选课程冲突']:
            r = requests.post(url, params=form, headers=header, cookies=cookie)
            # print(r.text)
            print(class_dict["KCH"], class_dict["KCM"], end='\t')
            print("选课", end='\t')
            msg = r.json()["msg"]
            print(msg)
    else:
        r = requests.post(url, params=form, headers=header, cookies=cookie)
        # print(r.text)
        print(class_dict["KCH"], class_dict["KCM"], end='\t')
        print(class_dict["SKJS"], end='\t')
        print("选课", end='\t')
        msg = r.json()["msg"]
        print(msg)


def dele(j, class_dict, cookie, batch, always=1, category=0):

    header = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44",
        "batchId": batch,
        "Authorization": j["data"]["token"]
    }

    # url1 = 'https://xk.xidian.edu.cn/xsxk/volunteer/list/choose'
    # form1 = {
    #     "clazzType": "TJKC",
    #     "clazzId": clazzId
    # }
    # r1 = requests.post(url1, params=form1, headers=header)
    # print(r1.text)

    # time.sleep(1)

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
            r = requests.post(url, params=form, headers=header, cookies=cookie)
            # print(r.text)
            print(class_dict["KCH"], class_dict["KCM"], end='\t')
            print("退课", end='\t')
            msg = r.json()["msg"]
            print(msg)
    else:
        r = requests.post(url, params=form, headers=header, cookies=cookie)
        # print(r.text)
        print(class_dict["KCH"], class_dict["KCM"], end='\t')
        print(class_dict["SKJS"], end='\t')
        print("退课", end='\t')
        msg = r.json()["msg"]
        print(msg)


if __name__ == '__main__':
    pass
    # 去运行 xk_main.py

