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


if __name__ == '__main__':
    print('{:-^30}'.format(""))
    print('{: ^30}'.format("Welcome"))
    print('{:-^30}'.format(""))
    # main()
    clazz = [
        {
            "KCH": "FL006121",
            "KXH": ""
        }
    ]
    del_1(1, kc=clazz, always=0)
    add_1(1, kc=clazz, always=0)
    print("Done")
