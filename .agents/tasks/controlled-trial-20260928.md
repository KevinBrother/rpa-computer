# 真实桌面黑盒验收：一次独立试验
只能通过computer工具、截图和键鼠操作，不使用任何其他接口。窗口外不要点击；不要打开编辑器、文件、终端或浏览器。看见锁屏立即停止。
1. computer_open后computer_observe，找标题为Computer Use Acceptance的窗口；读出当前Trial编号及NONCE完整文字，并描述四个图形各自颜色形状。NONCE必须从实际截图读取，不能猜测。
2. 找到唯一绿色圆形，使用图像像素坐标点击，观察并报告状态条是否显示TARGET HIT。
3. 在窗口的文本输入框输入窗口上印出的完整样例文字，点击Check text，获取截图并报告实际校验状态。
4. 结束前调用computer_close，报告本次实际Trial编号、NONCE、命中状态和文本状态。失败要如实记录。
每次computer_step的based_on用最近一次观察的observation_id；step如果返回了新图和observation则以后用新ID。每个步骤由截图判断结果，不自评代替证据。
