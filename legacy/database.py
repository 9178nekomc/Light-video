from flask_sqlalchemy import SQLAlchemy
from flask_login import UserMixin
from werkzeug.security import generate_password_hash, check_password_hash
from datetime import datetime
import os
from cryptography.fernet import Fernet

db = SQLAlchemy()

# 数据库加密
class DatabaseEncryption:
    def __init__(self):
        key_file = 'encryption.key'
        if os.path.exists(key_file):
            with open(key_file, 'rb') as f:
                self.key = f.read()
        else:
            self.key = Fernet.generate_key()
            with open(key_file, 'wb') as f:
                f.write(self.key)
        self.cipher = Fernet(self.key)
    
    def encrypt(self, data):
        if data is None:
            return None
        return self.cipher.encrypt(data.encode())
    
    def decrypt(self, data):
        if data is None:
            return None
        return self.cipher.decrypt(data).decode()

encryption = DatabaseEncryption()

# 弹幕模型 - 极简版
class Danmaku(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    video_id = db.Column(db.Integer, db.ForeignKey('video.id', ondelete='CASCADE'), nullable=False)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=True)
    content = db.Column(db.String(100), nullable=False)
    time = db.Column(db.Float, nullable=False)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    
    # 关系
    video = db.relationship('Video', foreign_keys=[video_id], back_populates='danmakus')
    user = db.relationship('User', foreign_keys=[user_id], back_populates='danmakus')
    
    def to_dict(self):
        return {
            'id': self.id,
            'video_id': self.video_id,
            'content': self.content,
            'time': self.time,
            'username': self.user.username if self.user else '游客',
            'user_avatar': self.user.avatar if self.user else 'default_avatar.png'
        }

# 好友关系模型
class Friendship(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    friend_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    status = db.Column(db.String(20), default='pending')  # pending, accepted, rejected
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    updated_at = db.Column(db.DateTime, default=datetime.utcnow, onupdate=datetime.utcnow)
    
    __table_args__ = (db.UniqueConstraint('user_id', 'friend_id', name='unique_friendship'),)

# 私信模型
class Message(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    sender_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    receiver_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    content = db.Column(db.Text, nullable=False)
    is_read = db.Column(db.Boolean, default=False)
    message_type = db.Column(db.String(20), default='text')  # text, video_share
    video_id = db.Column(db.Integer, db.ForeignKey('video.id', ondelete='CASCADE'), nullable=True)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    
    sender = db.relationship('User', foreign_keys=[sender_id], back_populates='sent_messages')
    receiver = db.relationship('User', foreign_keys=[receiver_id], back_populates='received_messages')
    video = db.relationship('Video', foreign_keys=[video_id], back_populates='messages')

# 通知模型
class Notification(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    type = db.Column(db.String(50), nullable=False)  # like, favorite, comment, friend_request, message
    content = db.Column(db.Text, nullable=False)
    is_read = db.Column(db.Boolean, default=False)
    related_id = db.Column(db.Integer)  # 相关的ID（视频ID、用户ID等）
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    
    user = db.relationship('User', foreign_keys=[user_id], back_populates='notifications')

# 用户模型
class User(UserMixin, db.Model):

    id = db.Column(db.Integer, primary_key=True)
    username = db.Column(db.String(80, collation='NOCASE'), unique=True, nullable=False)
    email = db.Column(db.String(120, collation='NOCASE'), unique=True, nullable=False)
    password_hash = db.Column(db.String(200), nullable=False)
    avatar = db.Column(db.String(200), default='default_avatar.png')
    bio = db.Column(db.String(500), default='')
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    oauth_accounts = db.relationship('OAuthAccount', back_populates='user', lazy=True, cascade='all, delete-orphan')
    
    # 关系
    videos = db.relationship('Video', back_populates='author', lazy=True, cascade='all, delete-orphan')
    likes = db.relationship('Like', back_populates='user', lazy=True, cascade='all, delete-orphan')
    favorites = db.relationship('Favorite', back_populates='user', lazy=True, cascade='all, delete-orphan')
    comments = db.relationship('Comment', back_populates='author', lazy=True, cascade='all, delete-orphan')
    danmakus = db.relationship('Danmaku', back_populates='user', lazy=True, cascade='all, delete-orphan')
    sent_messages = db.relationship('Message', foreign_keys='Message.sender_id', back_populates='sender', lazy=True, cascade='all, delete-orphan')
    received_messages = db.relationship('Message', foreign_keys='Message.receiver_id', back_populates='receiver', lazy=True, cascade='all, delete-orphan')
    notifications = db.relationship('Notification', back_populates='user', lazy=True, cascade='all, delete-orphan')
    
    def set_password(self, password):
        self.password_hash = generate_password_hash(password)
    
    def check_password(self, password):
        return check_password_hash(self.password_hash, password)
    
    # 好友相关方法
    def add_friend(self, friend):
        """发送好友请求"""
        # 检查是否已经是好友
        if self.is_friend(friend):
            return False
        
        # 检查是否已经存在待处理的好友请求
        existing = Friendship.query.filter(
            ((Friendship.user_id == self.id) & (Friendship.friend_id == friend.id)) |
            ((Friendship.user_id == friend.id) & (Friendship.friend_id == self.id))
        ).first()
        
        if existing:
            return False
        
        # 创建新的好友请求
        friendship = Friendship(user_id=self.id, friend_id=friend.id, status='pending')
        db.session.add(friendship)
        
        # 创建通知
        notification = Notification(
            user_id=friend.id,
            type='friend_request',
            content=f'{self.username} 请求添加你为好友',
            related_id=self.id
        )
        db.session.add(notification)
        db.session.commit()
        return True
    
    def accept_friend(self, friend):
        friendship = Friendship.query.filter_by(
            user_id=friend.id, 
            friend_id=self.id, 
            status='pending'
        ).first()
        if friendship:
            friendship.status = 'accepted'
            
            friendship2 = Friendship(user_id=self.id, friend_id=friend.id, status='accepted')
            db.session.add(friendship2)
            
            notification = Notification(
                user_id=friend.id,
                type='friend_request_accepted',
                content=f'{self.username} 接受了你的好友请求',
                related_id=self.id
            )
            db.session.add(notification)
            db.session.commit()
            return True
        return False
    
    def reject_friend(self, friend):
        friendship = Friendship.query.filter_by(
            user_id=friend.id, 
            friend_id=self.id, 
            status='pending'
        ).first()
        if friendship:
            friendship.status = 'rejected'
            db.session.commit()
            return True
        return False
    
    def remove_friend(self, friend):
        Friendship.query.filter(
            ((Friendship.user_id == self.id) & (Friendship.friend_id == friend.id)) |
            ((Friendship.user_id == friend.id) & (Friendship.friend_id == self.id))
        ).delete()
        db.session.commit()
    
    def is_friend(self, user):
        return Friendship.query.filter(
            ((Friendship.user_id == self.id) & (Friendship.friend_id == user.id) & (Friendship.status == 'accepted')) |
            ((Friendship.user_id == user.id) & (Friendship.friend_id == self.id) & (Friendship.status == 'accepted'))
        ).first() is not None
    
    def get_friends(self):
        friendships = Friendship.query.filter(
            ((Friendship.user_id == self.id) | (Friendship.friend_id == self.id)) &
            (Friendship.status == 'accepted')
        ).all()
        
        friends = []
        for f in friendships:
            if f.user_id == self.id:
                friend = User.query.get(f.friend_id)
            else:
                friend = User.query.get(f.user_id)
            if friend:
                friends.append(friend)
        return list(set(friends))
    
    def get_pending_requests(self):
        pending = Friendship.query.filter_by(
            friend_id=self.id, 
            status='pending'
        ).all()
        return [User.query.get(f.user_id) for f in pending]
    
    def send_message(self, receiver, content, message_type='text', video_id=None):
        message = Message(
            sender_id=self.id,
            receiver_id=receiver.id,
            content=content,
            message_type=message_type,
            video_id=video_id
        )
        db.session.add(message)
        
        notification = Notification(
            user_id=receiver.id,
            type='new_message',
            content=f'{self.username} 给你发了一条消息',
            related_id=self.id
        )
        db.session.add(notification)
        db.session.commit()
        return message
    
    def get_unread_messages_count(self):
        return Message.query.filter_by(receiver_id=self.id, is_read=False).count()
    
    def get_conversations(self):
        sent = db.session.query(Message.receiver_id).filter_by(sender_id=self.id).distinct()
        received = db.session.query(Message.sender_id).filter_by(receiver_id=self.id).distinct()
        user_ids = set([r[0] for r in sent] + [r[0] for r in received])
        
        conversations = []
        for uid in user_ids:
            user = User.query.get(uid)
            if user:
                last_message = Message.query.filter(
                    ((Message.sender_id == self.id) & (Message.receiver_id == uid)) |
                    ((Message.sender_id == uid) & (Message.receiver_id == self.id))
                ).order_by(Message.created_at.desc()).first()
                
                unread_count = Message.query.filter_by(
                    sender_id=uid, 
                    receiver_id=self.id, 
                    is_read=False
                ).count()
                
                conversations.append({
                    'user': user,
                    'last_message': last_message,
                    'unread_count': unread_count
                })
        
        return sorted(conversations, key=lambda x: x['last_message'].created_at if x['last_message'] else datetime.min, reverse=True)

# 视频模型
class Video(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    title = db.Column(db.String(200), nullable=False)
    description = db.Column(db.Text)
    video_file = db.Column(db.String(200), nullable=False)
    cover_image = db.Column(db.String(200))
    views = db.Column(db.Integer, default=0)
    danmaku_count = db.Column(db.Integer, default=0)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    
    # 关系
    author = db.relationship('User', foreign_keys=[user_id], back_populates='videos')
    likes = db.relationship('Like', back_populates='video', lazy=True, cascade='all, delete-orphan')
    favorites = db.relationship('Favorite', back_populates='video', lazy=True, cascade='all, delete-orphan')
    comments = db.relationship('Comment', back_populates='video', lazy=True, cascade='all, delete-orphan')
    danmakus = db.relationship('Danmaku', back_populates='video', lazy=True, cascade='all, delete-orphan')
    messages = db.relationship('Message', back_populates='video', lazy=True, cascade='all, delete-orphan')
    
    @property
    def like_count(self):
        return Like.query.filter_by(video_id=self.id).count()
    
    @property
    def favorite_count(self):
        return Favorite.query.filter_by(video_id=self.id).count()
    
    @property
    def comment_count(self):
        return Comment.query.filter_by(video_id=self.id).count()

# 点赞模型
class Like(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    video_id = db.Column(db.Integer, db.ForeignKey('video.id', ondelete='CASCADE'), nullable=False)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    
    __table_args__ = (db.UniqueConstraint('user_id', 'video_id', name='unique_user_video_like'),)
    
    user = db.relationship('User', back_populates='likes')
    video = db.relationship('Video', back_populates='likes')

# 收藏模型
class Favorite(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    video_id = db.Column(db.Integer, db.ForeignKey('video.id', ondelete='CASCADE'), nullable=False)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    
    __table_args__ = (db.UniqueConstraint('user_id', 'video_id', name='unique_user_video_favorite'),)
    
    user = db.relationship('User', back_populates='favorites')
    video = db.relationship('Video', back_populates='favorites')

# 评论模型
class Comment(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    content = db.Column(db.Text, nullable=False)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    video_id = db.Column(db.Integer, db.ForeignKey('video.id', ondelete='CASCADE'), nullable=False)
    
    author = db.relationship('User', back_populates='comments')
    video = db.relationship('Video', back_populates='comments')
    
    def to_dict(self):
        return {
            'id': self.id,
            'content': self.content,
            'created_at': self.created_at.strftime('%Y-%m-%d %H:%M:%S'),
            'username': self.author.username,
            'user_avatar': self.author.avatar
        }
    
    # 第三方登录关联模型
class OAuthAccount(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    user_id = db.Column(db.Integer, db.ForeignKey('user.id', ondelete='CASCADE'), nullable=False)
    provider = db.Column(db.String(50), nullable=False)  # 'ljworld'
    provider_user_id = db.Column(db.String(100), nullable=False)
    created_at = db.Column(db.DateTime, default=datetime.utcnow)
    
    __table_args__ = (db.UniqueConstraint('provider', 'provider_user_id', name='unique_oauth_account'),)
    
    # 关系
    user = db.relationship('User', back_populates='oauth_accounts')

# 在 User 模型中添加关系
# 找到 User 类，在现有关系后添加：
# oauth_accounts = db.relationship('OAuthAccount', back_populates='user', lazy=True, cascade='all, delete-orphan')