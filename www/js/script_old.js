$(document).ready(function() {
	navigator.geolocation.watchPosition(function(position) {
		var latitude = position.coords.latitude;
		var longitude = position.coords.longitude;
		console.log(location);
		console.log(latitude);
	var socket = io.connect('http://localhost');
	socket.on('meOnMap',function(latitude,longitude){
		console.log('Mine:'+latitude+','+longitude);
		var gLatLng = new google.maps.LatLng(latitude, longitude);
		var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
		var mapOptions = {
			center: gLatLng,
			zoom: 18,
			mapTypeId: google.maps.MapTypeId.ROADMAP,
			draggable: false,
			zoomControl: false,
			scrollwheel: false,
			disableDoubleClickZoom: true,
			disableDefaultUI: true,
		};
		var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
		var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
		var yourLocation = {
			path: google.maps.SymbolPath.CIRCLE,
			strokeColor: "#4c8f2a",
			strokeWeight: 2,
			fillColor: "#a2e346",
			fillOpacity: 1,
			scale: 6,
		};
		var mapUserName = new MapLabel({
			text: 'Anonymous User',
			position: nameLatLng,
			map: map,
			fontSize: 14,
			fontColor: "#ffffff",
			strokeWeight: 4,
			strokeColor: "4c8f2a",
			fontFamily: "Avenir",
		});
		var drawCircle = new google.maps.Marker({
			position: gLatLng,
			icon: yourLocation,
			map: map,
		});
		var infoBubble = new InfoBubble({
			position: gLatLng,
			maxWidth: 300,
			backgroundColor: 'rgba(255,255,255,0.8)',
			backgroundClassName: 'bubble',
			hideCloseButton: true,
			shadowStyle: 0,
		});
		$("#update").click(function(){
			mapUserName.set('text', $("#userName").val());
			infoBubble.addTab($("#userName").val() + ':', status);
			infoBubble.open(map);
		});
		socket.on('reDoMap',function(latitude,longitude){
			console.log('Others:'+latitude+','+longitude);
			var gLatLng = new google.maps.LatLng(latitude, longitude);
			var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
			var othersLocation = {
				path: google.maps.SymbolPath.CIRCLE,
				strokeColor: "#000",
				strokeWeight: 2,
				fillColor: "#ff0000",
				fillOpacity: 1, 
				scale: 6,
			};
			var mapUserName = new MapLabel({
				text: 'Anonymous User',
				position: nameLatLng,
				map: map,
				fontSize: 14,
				fontColor: "#ffffff",
				strokeWeight: 4,
				strokeColor: "#000000",
				fontFamily: "Avenir",
			});
			var drawCircle = new google.maps.Marker({
				position: gLatLng,
				icon: othersLocation,
				map: map,
			});
		});
	});
	function drawMe() {
		navigator.geolocation.watchPosition(function(position) {
			var latitude = position.coords.latitude;
			var longitude = position.coords.longitude;
			socket.emit('addToMap',latitude,longitude);
			var gLatLng = new google.maps.LatLng(latitude, longitude);
			var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
			var mapOptions = {
				center: gLatLng,
				zoom: 18,
				mapTypeId: google.maps.MapTypeId.ROADMAP,
				draggable: false,
				zoomControl: false,
				scrollwheel: false,
				disableDoubleClickZoom: true,
				disableDefaultUI: true,
			};
			var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
			var yourLocation = {
				path: google.maps.SymbolPath.CIRCLE,
				strokeColor: "#4c8f2a",
				strokeWeight: 2,
				fillColor: "#a2e346",
				fillOpacity: 1,
				scale: 6,
			};
			var drawCircle = new google.maps.Marker({
				position: gLatLng,
				icon: yourLocation,
				map: map,
			});
			var mapUserName = new MapLabel({
				text: 'Anonymous User',
				position: nameLatLng,
				map: map,
				fontSize: 14,
				fontColor: "#ffffff",
				strokeWeight: 4,
				strokeColor: "4c8f2a",
				fontFamily: "Avenir",
			});

			var infoBubble = new InfoBubble({
				position: gLatLng,
				maxWidth: 300,
				backgroundColor: 'rgba(255,255,255,0.8)',
				backgroundClassName: 'bubble',
				hideCloseButton: true,
				shadowStyle: 0,
			});

			var status = $("#status-update").val();

			$("#update").click(function(){
				mapUserName.set('text', $("#userName").val());
				infoBubble.addTab($("#userName").val() + ':', status);
				infoBubble.open(map);
				socket.emit('update',$("#userName").val(),status);
			});
		});
	};
	drawMe();

	function resizeText() {
		var windowHeight = $(window).height();
		var windowWidth = $(window).width();
		$("footer").css({
			'padding-top': windowHeight * 0.15 * 0.1 + 'px',
		});
	};
	resizeText();

	$(window).resize(function(){
		drawMe();
		resizeText();
	});
});