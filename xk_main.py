from login import *
import time


def main():
    print('{:-^30}'.format(""))
    print('{: ^30}'.format("Welcome"))
    print('{:-^30}'.format(""))

    with open("conf.json", 'r', encoding="utf-8") as f:
        conf = json.load(f)  # 加载配置
    a, b = login(conf)
    show_msg(a)  # 显示个人信息
    lst = get_class(a, conf)

    for kch in conf["KCH"]:
        for i in lst["data"]["rows"]:
            if i["KCH"] == kch:
                add(a, i, cookie=b)
                break

    for kcm in conf["KCM"]:
        for i in lst["data"]["rows"]:
            if i["KCM"] == kcm:
                add(a, i, cookie=b)
                break



if __name__ == '__main__':
    main()