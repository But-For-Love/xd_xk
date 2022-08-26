# -*- coding: utf-8 -*-
from func import *
import time


def main():
    with open("conf.json", 'r', encoding="utf-8") as f:
        conf = json.load(f)  # 加载配置

    a, b = login(conf)
    cat = conf["bx_or_xx"]
    batch = show_msg(a)  # 显示个人信息
    # choose_Batch(a, batch=batch)
    lst = get_class(a, conf, batch=batch, category=cat)

    if cat == 0:    # 必修课
        for kch in conf["bx"]:  # 课程号索引
            for i in lst["data"]["rows"]:
                if i["KCH"] == kch["KCH"]:
                    for j in i["tcList"]:
                        if j["KXH"] == kch["KXH"]:
                            add(a, j, cookie=b, batch=batch, category=cat)
                            # add(a, j, cookie=b, batch="5ed2a2e6bb97425b8ae7d8ce138283b5", category=cat)
                            break
    elif cat == 1:  # 选修课
        for kch in conf["xx"]:  # 课程号索引
            for i in lst["data"]["rows"]:
                if i["KCH"] == kch["KCH"]:
                    add(a, i, cookie=b, batch=batch, category=cat)
                    break
    # for kcm in conf["KCM"]:
    #     for i in lst["data"]["rows"]:
    #         if i["KCM"] == kcm:
    #             add(a, i, cookie=b, batch=batch, category=cat)
    #             break


def add_1(bx_or_xx, kc=[{}], always=1):
    """
    :param bx_or_xx: 必修_or_选修: 0-必修  1-选修
    :param kc:      !!!注意格式!!!
    :param always: 是否连续选课 1-是  0-否
    :return:
    """
    with open("conf.json", 'r', encoding="utf-8") as f:
        conf = json.load(f)  # 加载配置
    a, b = login(conf)

    batch = show_msg(a)  # 显示个人信息
    cat = bx_or_xx
    lst = get_class(a, conf, batch=batch, category=cat)

    if cat == 0:
        for kch in kc:  # 课程号索引
            for i in lst["data"]["rows"]:
                if i["KCH"] == kch["KCH"]:
                    for j in i["tcList"]:
                        if j["KXH"] == kch["KXH"]:
                            add(a, j, cookie=b, batch=batch, category=cat, always=always)
                            break

    elif cat == 1:
        for kch in kc:  # 课程号索引
            for i in lst["data"]["rows"]:
                if i["KCH"] == kch["KCH"]:
                    add(a, i, cookie=b, batch=batch, category=cat, always=always)
                    break


    # for kch in KCH:
    #     for i in lst["data"]["rows"]:
    #         if i["KCH"] == kch:
    #             add(a, i, cookie=b, batch=batch, category=cat, always=always)
    #             break
    #
    # for kcm in KCM:
    #     for i in lst["data"]["rows"]:
    #         if i["KCM"] == kcm:
    #             add(a, i, cookie=b, batch=batch, category=cat, always=always)
    #             break


def del_1(bx_or_xx, kc=[{}], always=1):
    """
    :param bx_or_xx: 必修_or_选修: 0-必修  1-选修
    :param KCM: 课程名，例如 ["通信原理"]
    :param KCH: 课程号，例如 ["TE204004"]
    :param always: 是否连续选课 1-是  0-否
    :return:
    """
    with open("conf.json", 'r', encoding="utf-8") as f:
        conf = json.load(f)  # 加载配置
    a, b = login(conf)

    batch = show_msg(a)  # 显示个人信息
    cat = bx_or_xx
    lst = get_class(a, conf, batch=batch, category=cat)

    if cat == 0:
        for kch in kc:  # 课程号索引
            for i in lst["data"]["rows"]:
                if i["KCH"] == kch["KCH"]:
                    for j in i["tcList"]:
                        if j["KXH"] == kch["KXH"]:
                            dele(a, j, cookie=b, batch=batch, category=cat, always=always)
                            break

    elif cat == 1:
        for kch in kc:  # 课程号索引
            for i in lst["data"]["rows"]:
                if i["KCH"] == kch["KCH"]:
                    dele(a, i, cookie=b, batch=batch, category=cat, always=always)
                    break

    # for kch in KCH:
    #     for i in lst["data"]["rows"]:
    #         if i["KCH"] == kch:
    #             dele(a, i, cookie=b, batch=batch, category=cat, always=always)
    #             break
    #
    # for kcm in KCM:
    #     for i in lst["data"]["rows"]:
    #         if i["KCM"] == kcm:
    #             dele(a, i, cookie=b, batch=batch, category=cat, always=always)
    #             break


def check():
    """
    按照课程号检查
    这里代码特指选修，必修的表单格式不同
    """
    with open("conf.json", 'r', encoding="utf-8") as f:
        conf = json.load(f)  # 加载配置
    a, b = login(conf)

    batch = show_msg(a)  # 显示个人信息
    cat = conf["bx_or_xx"]
    k = 0
    while 1:
        k += 1
        lst = get_class(a, conf, batch=batch, category=cat)["data"]["rows"]
        for i in lst:
            if i["KCH"] in ["EY226022", "EY226023", "EY226024", "EY226025",
                            "EY226026", "EY226027", "EY226028", "EY226029", "EY226030",
                            "EY226031", "EY226032", "EY226035", "EY226036", "EY226037",
                            "EY226038", "EY226039", "EY226041", "EY226042",
                            "EY226043", "EY226044", "EY226045", "EY226046", "EY226047"] and i["SFYX"] == "0":
                print(i["KCM"], i["numberOfSelected"], i["classCapacity"])
                if i["numberOfSelected"] < i["classCapacity"]:
                    print(i["KXH"], i["KCM"])
                    add(a, i, b, batch, category=1, always=0)
        print("第", k, "次", k*"-")
        k = k % 10
        time.sleep(0.5)


if __name__ == '__main__':
    print('{:-^30}'.format(""))
    print('{: ^30}'.format("Welcome"))
    print('{:-^30}'.format(""))
    # main()
    # clazz = [
    #     {
    #         "KCH": "ZH226044",
    #         "KXH": ""
    #     },
    #     {
    #         "KCH": "ZH226041",
    #         "KXH": ""
    #     }
    # ]
    # del_1(0, kc=clazz, always=1)
    # add_1(1, kc=clazz, always=1)

    # with异常处理
    # while 1:
    #     try:
    #         check()
    #     except:
    #         pass

    print("Done")
