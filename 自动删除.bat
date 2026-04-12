@echo off
chcp 65001 >nul
title Video Delete Tool
setlocal enabledelayedexpansion

:: 设置颜色
color 0F

:: 检查Python环境
python --version >nul 2>&1
if errorlevel 1 (
    echo Error: Python not found, please install Python first
    pause
    exit /b 1
)

:: 检查是否在正确的目录
if not exist "app.py" (
    echo Error: Please run this script in the video site root directory
    echo Current directory: %cd%
    pause
    exit /b 1
)

:: 检查数据库文件
if not exist "video_site.db" (
    echo Database file not found!
    echo Files in current directory:
    dir /b
    echo.
    echo Please check if video_site.db exists
    pause
    exit /b 1
) else (
    echo Database file found: video_site.db
    timeout /t 2 /nobreak >nul
)

:: 主菜单
:main_menu
cls
echo ========================================
echo         Video Delete Tool v2.0
echo ========================================
echo  1. List all videos
echo  2. Select and delete videos
echo  3. Delete videos by username
echo  4. Delete by video ID
echo  5. Exit
echo ========================================
echo.

set /p choice="Please select (1-5): "

if "%choice%"=="1" goto list_videos
if "%choice%"=="2" goto select_video
if "%choice%"=="3" goto delete_by_user
if "%choice%"=="4" goto delete_by_id
if "%choice%"=="5" exit /b 0
goto main_menu

:list_videos
cls
echo Getting video list...
echo.

:: 创建Python脚本来获取视频列表
(
echo import sys
echo import os
echo from app import app
echo from database import db, Video, User
echo.
echo def main():
echo     try:
echo         with app.app_context():
echo             videos = Video.query.order_by(Video.id.desc()).all()
echo             if not videos:
echo                 print("="*60)
echo                 print("No videos in database")
echo                 print("="*60)
echo                 return 0
echo             
echo             print("="*80)
echo             print(f"{'ID':^6} {'Title':^30} {'Author':^15} {'Views':^10} {'Likes':^8}")
echo             print("="*80)
echo             
echo             for v in videos:
echo                 author = User.query.get(v.user_id)
echo                 author_name = author.username if author else "Unknown"
echo                 title = v.title[:28] + ".." if len(v.title) > 28 else v.title
echo                 print(f"{v.id:^6} {title:^30} {author_name:^15} {v.views:^10} {v.like_count:^8}")
echo             
echo             print("="*80)
echo             print(f"Total: {len(videos)} videos")
echo             return 0
echo     except Exception as e:
echo         print(f"Error: {str(e)}")
echo         return 1
echo.
echo if __name__ == '__main__':
echo     sys.exit(main())
) > %temp%\list_videos.py

python %temp%\list_videos.py
del %temp%\list_videos.py

echo.
pause
goto main_menu

:select_video
cls
echo Loading video list...
echo.

:: 创建Python脚本来获取视频列表并显示选择界面
(
echo import sys
echo import os
echo import msvcrt
echo from app import app
echo from database import db, Video, User
echo.
echo class VideoSelector:
echo     def __init__(self):
echo         self.videos = []
echo         self.selected_indices = set()
echo         self.current_index = 0
echo         self.load_videos()
echo     
echo     def load_videos(self):
echo         try:
echo             with app.app_context():
echo                 videos = Video.query.order_by(Video.id.desc()).all()
echo                 for v in videos:
echo                     author = User.query.get(v.user_id)
echo                     author_name = author.username if author else "Unknown"
echo                     self.videos.append({
echo                         'id': v.id,
echo                         'title': v.title,
echo                         'author': author_name,
echo                         'views': v.views,
echo                         'likes': v.like_count
echo                     })
echo         except Exception as e:
echo             print(f"Failed to load videos: {e}")
echo             sys.exit(1)
echo     
echo     def clear_screen(self):
echo         os.system('cls' if os.name == 'nt' else 'clear')
echo     
echo     def draw(self):
echo         self.clear_screen()
echo         print("="*80)
echo         print("                    Video Delete Selector")
echo         print("="*80)
echo         print("  ↑/↓:Move  Space:Select/Deselect  Enter:Confirm  ESC:Exit")
echo         print("-"*80)
echo         
echo         if not self.videos:
echo             print("                    No videos available")
echo         else:
echo             for i, v in enumerate(self.videos):
echo                 prefix = "->" if i == self.current_index else "  "
echo                 marker = "[X]" if i in self.selected_indices else "[ ]"
echo                 title = v['title'][:40] + "..." if len(v['title']) > 40 else v['title']
echo                 print(f"{prefix} {marker} ID:{v['id']:4d} | {title:44s} | {v['author']:12s} | {v['views']:6d} views")
echo         
echo         print("-"*80)
echo         print(f"Selected: {len(self.selected_indices)} videos")
echo         print("="*80)
echo     
echo     def run(self):
echo         if not self.videos:
echo             print("\nNo videos to select")
echo             input("Press Enter to return...")
echo             return []
echo         
echo         while True:
echo             self.draw()
echo             
echo             key = msvcrt.getch()
echo             if key == b'\xe0':  # Special keys
echo                 key = msvcrt.getch()
echo                 if key == b'H':  # Up arrow
echo                     self.current_index = (self.current_index - 1) %% len(self.videos)
echo                 elif key == b'P':  # Down arrow
echo                     self.current_index = (self.current_index + 1) %% len(self.videos)
echo             elif key == b' ':  # Space
echo                 if self.current_index in self.selected_indices:
echo                     self.selected_indices.remove(self.current_index)
echo                 else:
echo                     self.selected_indices.add(self.current_index)
echo             elif key == b'\r':  # Enter
echo                 if self.selected_indices:
echo                     self.draw()
echo                     print("\n" + "="*60)
echo                     print("Videos to delete:")
echo                     for i in sorted(self.selected_indices):
echo                         v = self.videos[i]
echo                         print(f"  - {v['title']} (ID: {v['id']})")
echo                     print("="*60)
echo                     print("Press Y to confirm, any other key to cancel")
echo                     
echo                     confirm = msvcrt.getch().lower()
echo                     if confirm == b'y':
echo                         return [self.videos[i]['id'] for i in self.selected_indices]
echo                 else:
echo                     print("\nPlease select at least one video!")
echo                     msvcrt.getch()
echo             elif key == b'\x1b':  # ESC
echo                 return []
echo     
echo def main():
echo     selector = VideoSelector()
echo     selected_ids = selector.run()
echo     
echo     if selected_ids:
echo         # Save selected IDs to file
echo         with open(r'%temp%\selected_ids.txt', 'w', encoding='utf-8') as f:
echo             for vid in selected_ids:
echo                 f.write(str(vid) + '\n')
echo         print(f"\nSelected {len(selected_ids)} videos")
echo     else:
echo         print("\nNo videos selected")
echo     
echo     return 0
echo.
echo if __name__ == '__main__':
echo     sys.exit(main())
) > %temp%\video_selector.py

:: 运行选择器
python %temp%\video_selector.py

:: 检查是否有选中的视频
if exist "%temp%\selected_ids.txt" (
    cls
    echo Deleting selected videos...
    echo.
    
    set delete_count=0
    for /f "usebackq delims=" %%i in (`type "%temp%\selected_ids.txt"`) do (
        set /a delete_count+=1
        echo Deleting video ID: %%i
        call :delete_video %%i
    )
    
    echo.
    echo Complete! Deleted !delete_count! videos.
    del "%temp%\selected_ids.txt"
) else (
    echo No videos selected
)

pause
goto main_menu

:delete_by_id
cls
echo Enter video IDs to delete (separate multiple IDs with spaces):
set /p ids="Video IDs: "

if "%ids%"=="" (
    echo Video ID cannot be empty
    pause
    goto main_menu
)

cls
echo Deleting videos...
echo.

set delete_count=0
for %%i in (%ids%) do (
    set /a delete_count+=1
    echo Deleting video ID: %%i
    call :delete_video %%i
)

echo.
echo Complete! Deleted !delete_count! videos.
pause
goto main_menu

:delete_by_user
cls
echo Enter username to delete their videos:
set /p username="Username: "

if "%username%"=="" (
    echo Username cannot be empty
    pause
    goto main_menu
)

:: 创建删除脚本
(
echo import sys
echo from app import app
echo from database import db, User, Video, Like, Favorite, Comment, Message, Notification
echo import os
echo.
echo def main():
echo     try:
echo         with app.app_context():
echo             user = User.query.filter_by(username="%username%").first()
echo             if not user:
echo                 print(f"Error: User '{username}' not found")
echo                 return 1
echo             
echo             videos = Video.query.filter_by(user_id=user.id).all()
echo             if not videos:
echo                 print(f"User '{user.username}' has no videos")
echo                 return 0
echo             
echo             print(f"Found {len(videos)} videos for user '{user.username}':")
echo             for v in videos:
echo                 print(f"  ID: {v.id} - {v.title}")
echo             
echo             confirm = input(f"\nDelete all videos for this user? (yes/no): ")
echo             if confirm.lower() != 'yes':
echo                 print("Cancelled")
echo                 return 0
echo             
echo             success = 0
echo             for video in videos:
echo                 try:
echo                     # Delete video file
echo                     video_path = os.path.join('uploads/videos', video.video_file)
echo                     if os.path.exists(video_path):
echo                         os.remove(video_path)
echo                         print(f"  ✓ Deleted video file: {video.video_file}")
echo                     
echo                     # Delete cover
echo                     if video.cover_image != 'default_cover.jpg':
echo                         cover_path = os.path.join('uploads/covers', video.cover_image)
echo                         if os.path.exists(cover_path):
echo                             os.remove(cover_path)
echo                             print(f"  ✓ Deleted cover: {video.cover_image}")
echo                     
echo                     # Delete related data
echo                     Notification.query.filter_by(related_id=video.id).delete()
echo                     Message.query.filter_by(video_id=video.id).delete()
echo                     db.session.delete(video)
echo                     success += 1
echo                     print(f"  ✓ Deleted: {video.title}")
echo                 except Exception as e:
echo                     print(f"  ✗ Failed to delete {video.title}: {str(e)}")
echo             
echo             db.session.commit()
echo             print(f"\nComplete! Deleted {success}/{len(videos)} videos")
echo             return 0
echo     except Exception as e:
echo         print(f"Error: {str(e)}")
echo         return 1
echo.
echo if __name__ == '__main__':
echo     sys.exit(main())
) > %temp%\delete_user_videos.py

python %temp%\delete_user_videos.py
del %temp%\delete_user_videos.py

pause
goto main_menu

:delete_video
set "video_id=%1"
(
echo import sys
echo from app import app
echo from database import db, Video, Like, Favorite, Comment, Message, Notification
echo import os
echo.
echo def main():
echo     try:
echo         with app.app_context():
echo             video = Video.query.get(%video_id%)
echo             if not video:
echo                 print(f"  Error: Video ID {video_id} not found")
echo                 return 1
echo             
echo             print(f"  Deleting: {video.title}")
echo             
echo             # Delete video file
echo             video_path = os.path.join('uploads/videos', video.video_file)
echo             if os.path.exists(video_path):
echo                 os.remove(video_path)
echo                 print(f"    ✓ Deleted video file")
echo             
echo             # Delete cover
echo             if video.cover_image != 'default_cover.jpg':
echo                 cover_path = os.path.join('uploads/covers', video.cover_image)
echo                 if os.path.exists(cover_path):
echo                     os.remove(cover_path)
echo                     print(f"    ✓ Deleted cover")
echo             
echo             # Delete related data
echo             Notification.query.filter_by(related_id=%video_id%).delete()
echo             Message.query.filter_by(video_id=%video_id%).delete()
echo             
echo             # Delete video
echo             db.session.delete(video)
echo             db.session.commit()
echo             print(f"    ✓ Deleted from database")
echo             return 0
echo     except Exception as e:
echo         print(f"    ✗ Delete failed: {str(e)}")
echo         db.session.rollback()
echo         return 1
echo.
echo if __name__ == '__main__':
echo     sys.exit(main())
) > %temp%\delete_video_%video_id%.py

python %temp%\delete_video_%video_id%.py
del %temp%\delete_video_%video_id%.py
goto :eof