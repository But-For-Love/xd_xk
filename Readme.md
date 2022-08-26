# _How To Install_

下载或者克隆项目到本地

进入文件夹目录

执行:
```commandline
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple -r requirements.txt
```
---

# _How To Use_


打开**conf.json**
```
{
  "ocr_captcha": "1",               是否使用自动识别验证码
  "debug": "0",                     是否输出调试文件
  "bx_or_xx": "0",                  0是必修，1是选修
  "bx": [                           必修课程
    {
      "KCH": "TE204003",            课程号
      "KXH": "02"                   课序号(小卡片左上角：[01])(必填)
    }
  ],
  "xx": [                           选修课程
    {
      "KCH": "FL006066"             课程号
    }
  ],
  "data": {
    "loginname": "username",        学号
    "password": "password",         密码
    "captcha": "xxxx",
    "uuid": "xxxx"
  }
}
```
将 **学号（username）密码（password）** 填入，然后保存

---
# _How To Run_
1、在conf.json中相应位置填入**课程号**或**课程名**，**可以填多个**

2、修改"bx_or_xx"的参数，0表示必修，1表示选修

**！单次只能选择一种类别（必修/选修）**

用任意IDE运行xk_main.py，或者拖动到cmd里面执行

---

# _How To Run _ 2_
为了提供一种更加灵活的选课方法，在xk_main.py中提供了
`add_1(bx_or_xx, kc=[{}], always=1)`
函数，优先级高于conf.json
```
参数:
 - bx_or_xx: 必修_or_选修 : 0_or_1
 - kc      : 课程
 如：
 clazz = [
        {
            "KCH": "TE204003"， 
            "KXH": "02"
        },
        {
            "KCH": "FL006121"，
            "KXH": ""
        },
    ]
 - always  : 是否连续选课
   - 为1表示，按照顺序，选课成功后才会进行下一门课
   - 为0表示，仅执行一次选课操作
   ```
若要使用add_1函数,将xk_main.py下面代码中main()注释掉，使用原注释部分
```commandline
if __name__ == '__main__':
    print('{:-^30}'.format(""))
    print('{: ^30}'.format("Welcome"))
    print('{:-^30}'.format(""))
    main()
    # clazz = [
    #     {
    #         "KCH": "FL006121"，
    #         "KXH": ""
    #     }
    # ]
    # del_1(1, kc=clazz, always=1)
    # add_1(1, kc=clazz, always=1)
    print("Done")
```
del_1函数同add_1

---
# Attention
 - 如果conf.json中没填学号密码，运行时可以手动输入
 - 如果运行时提示**验证码错误**，可以**重新运行**，或者**改为手动识别验证码(不建议)**
 - 手动输入验证码需要将图片关闭后，再输入
 - 通过对add_1/del_1函数的组合，可以同时实现必修和选修/选课和退课
 - 默认进入2020级所在的批次，如果要修改批次，可以修改func.py第90行中的"2020"
 - 必修课必填课序号，选修课可以不填课序号