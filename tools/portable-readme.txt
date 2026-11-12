CapCraft — โปรแกรมตัดต่อวิดีโอ (Windows 10 / 11, 64-bit)
==========================================================

วิธีใช้
-------
1. แตกไฟล์ zip ทั้งโฟลเดอร์ไว้ที่ไหนก็ได้ (เช่น Desktop หรือ D:\CapCraft)
   อย่าเปิดจากในไฟล์ zip ตรงๆ ต้องแตกออกมาก่อน
2. ดับเบิลคลิก CapCraft.exe
3. ถ้า Windows ขึ้นกรอบสีฟ้า "Windows protected your PC"
   ให้กด "More info" แล้วกด "Run anyway" (ขึ้นแค่ครั้งแรก เพราะโปรแกรมไม่ได้ซื้อลายเซ็นดิจิทัล)

ไม่ต้องติดตั้งอะไรเพิ่ม และไม่ต้องใช้อินเทอร์เน็ต
ไฟล์ ffmpeg.exe กับ ffprobe.exe ต้องอยู่โฟลเดอร์เดียวกับ CapCraft.exe (ใช้อ่านวิดีโอและ Export)

งานที่ทำจะเก็บไว้ที่  Documents\CapCraft Projects
การตั้งค่าเก็บไว้ที่   %APPDATA%\CapCraft

ซับอัตโนมัติ (Auto captions) — ไม่บังคับ
---------------------------------------
ต้องมีชุดเสริม "CapCraft-auto-captions" (ไฟล์ใหญ่ประมาณ 3 GB)
แตกแล้วจะได้โฟลเดอร์ชื่อ whisper ให้วางไว้ข้าง CapCraft.exe
แบบเร็วต้องใช้การ์ดจอ NVIDIA ถ้าไม่มี ให้เลือก "Processor (CPU)" ใน Menu > Settings

ลบโปรแกรม
---------
ลบโฟลเดอร์ที่แตกไว้ได้เลย (งานใน Documents\CapCraft Projects ยังอยู่)


CapCraft — video editor (Windows 10 / 11, 64-bit)
-------------------------------------------------
1. Unzip the whole folder anywhere (don't run it from inside the zip).
2. Double-click CapCraft.exe.
3. If Windows says "Windows protected your PC", click "More info", then "Run anyway".
Keep ffmpeg.exe and ffprobe.exe beside CapCraft.exe. Nothing else to install; no internet needed.
Auto captions need the separate "CapCraft-auto-captions" pack: put its whisper folder beside CapCraft.exe.

Licences: see the licences folder. CapCraft is MIT-licensed. FFmpeg is licensed under the
GNU GPL version 3 (licences\FFmpeg-GPLv3.txt); its source code is at https://ffmpeg.org
and the build used is from https://www.gyan.dev/ffmpeg/builds/.
